//! Choosing a rectangle by dragging over the screen.
//!
//! Every monitor gets a surface of its own: a webview window covering it,
//! showing a **still of that monitor taken a moment earlier**. Three
//! things fall out of freezing the screen instead of drawing a
//! transparent hole over it:
//!
//! * the selection can be cropped out of the still, so what the user saw
//!   while dragging is exactly what they get — no second capture, no
//!   window that closed in between;
//! * the surface itself can never appear in the result, which a live
//!   transparent overlay has to work to avoid;
//! * the dimming, the marching rectangle and the size readout are ordinary
//!   CSS over an image, so the part that has to look right is the part we
//!   can look at.
//!
//! One window per monitor rather than one stretched across the desktop:
//! a window that straddles displays behaves differently on every
//! platform, and macOS gives each display its own space. The surfaces
//! share one session — whichever one answers ends it, and they all come
//! down together.
//!
//! `finish` is keyed to the session that is actually running, so a stale
//! reply from a window that was already closed resolves nothing.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

use super::{Fractions, Frame, Rect, Screen};

/// Window labels are `capture-region-<index>`, which is how a command
/// knows which monitor is talking to it. Derived rather than stored: a
/// reply carries no more authority than its own label.
const LABEL_PREFIX: &str = "capture-region-";

pub fn label(index: usize) -> String {
    format!("{}{}", LABEL_PREFIX, index)
}

pub fn index_of(label: &str) -> Option<usize> {
    label.strip_prefix(LABEL_PREFIX)?.parse().ok()
}

/// Whether a window is one of the selection surfaces. The gate for the
/// commands only they may call.
pub fn is_surface(label: &str) -> bool {
    index_of(label).is_some()
}

struct Pending {
    surfaces: Vec<Surface>,
    answer: Option<oneshot::Sender<(usize, Option<Fractions>)>>,
}

/// What one surface draws, and what it is being drawn for.
///
/// The image is asked for by the window once it has mounted rather than
/// pushed to it, so there is no race between the window appearing and the
/// frame arriving. `recording` only changes the wording — but a surface
/// that says "drag to screenshot" and then starts recording is the kind
/// of surprise a screen tool cannot afford.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Surface {
    /// The frozen monitor, as a data URL
    pub image: String,
    pub recording: bool,
    /// Which monitor this is, and how many there are, so the surface can
    /// say "画面 2 / 3" instead of leaving the user to guess which of the
    /// dimmed screens it is on.
    pub index: usize,
    pub screens: usize,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// What a surface shows. Empty when no selection is running, which is the
/// honest answer to a window asking after its session ended.
pub fn surface(label: &str) -> Surface {
    let Some(index) = index_of(label) else {
        return Surface::default();
    };
    PENDING
        .lock()
        .ok()
        .and_then(|pending| {
            pending
                .as_ref()
                .and_then(|p| p.surfaces.get(index).cloned())
        })
        .unwrap_or_default()
}

/// End the current selection. `None` is a cancel — Escape, a right-click,
/// or a drag too small to be a rectangle. Which surface answered decides
/// which monitor the rectangle is on.
pub fn finish(label: &str, selection: Option<Fractions>) {
    let Some(index) = index_of(label) else {
        return;
    };
    let sender = PENDING
        .lock()
        .ok()
        .and_then(|mut pending| pending.as_mut().and_then(|p| p.answer.take()));
    if let Some(sender) = sender {
        let _ = sender.send((index, selection));
    }
}

/// True while the surfaces are up. The capture commands check it: two
/// sessions would each freeze a different moment and fight over the
/// pointer.
pub fn is_running() -> bool {
    PENDING.lock().map(|p| p.is_some()).unwrap_or(false)
}

/// What a selection resolved to: which monitor it is on, where it is
/// inside that monitor's frozen frame, and where it is on the screen. A
/// still is cropped out of the frame; a recording re-reads the screen
/// rectangle, frame after frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub screen: Screen,
    pub in_frame: Rect,
    pub on_screen: Rect,
}

/// Put the frozen monitors up and wait for a drag on one of them.
/// `Ok(None)` is a cancel, which is an ordinary way to end.
pub async fn select(
    app: &AppHandle,
    frames: &[Frame],
    recording: bool,
) -> Result<Option<Selection>, String> {
    if is_running() {
        return Err(crate::i18n::t("errors", "capture_busy").to_string());
    }
    if frames.is_empty() {
        return Err(crate::i18n::t("errors", "capture_failed").to_string());
    }

    let (sender, receiver) = oneshot::channel();
    {
        let mut surfaces = Vec::with_capacity(frames.len());
        for (index, (_, shot)) in frames.iter().enumerate() {
            surfaces.push(Surface {
                image: shot.to_data_url()?,
                recording,
                index,
                screens: frames.len(),
            });
        }
        let mut pending = PENDING.lock().map_err(|_| "capture state is poisoned")?;
        *pending = Some(Pending {
            surfaces,
            answer: Some(sender),
        });
    }

    let mut opened = 0;
    for (index, (screen, _)) in frames.iter().enumerate() {
        // A monitor whose surface will not open is one the user cannot
        // select on; the others are still worth having.
        if open_surface(app, index, *screen).await.is_ok() {
            opened += 1;
        }
    }
    if opened == 0 {
        close_surfaces(app, frames.len());
        clear();
        return Err(crate::i18n::t("errors", "capture_surface").to_string());
    }

    // The windows are closed here rather than by the page: whatever ended
    // the session — Escape, a drag, a failure — leaves nothing on screen.
    let answer = receiver.await.ok();
    close_surfaces(app, frames.len());
    clear();

    let Some((index, Some(fractions))) = answer else {
        return Ok(None);
    };
    let Some((screen, shot)) = frames.get(index) else {
        return Ok(None);
    };
    Ok(fractions.to_offsets(shot.width, shot.height).and_then(|in_frame| {
        Some(Selection {
            screen: *screen,
            in_frame,
            on_screen: fractions.to_screen(*screen)?,
        })
    }))
}

fn clear() {
    if let Ok(mut pending) = PENDING.lock() {
        *pending = None;
    }
}

fn close_surfaces(app: &AppHandle, count: usize) {
    for index in 0..count {
        if let Some(window) = app.get_webview_window(&label(index)) {
            let _ = window.close();
        }
    }
}

/// Build the surface for one monitor.
///
/// Created hidden and shown by the page (`capture_surface_ready`) once it
/// has the frozen frame painted: a window that appears before its image
/// does is a white flash over whatever the user was about to capture.
async fn open_surface(app: &AppHandle, index: usize, screen: Screen) -> Result<(), String> {
    let (done, built) = oneshot::channel();
    let handle = app.clone();

    app.run_on_main_thread(move || {
        let result = WebviewWindowBuilder::new(
            &handle,
            label(index),
            WebviewUrl::App("index.html#capture".into()),
        )
        .title("Conduit capture")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        // The surfaces own the keyboard while they are up: Escape has to
        // reach one of them, and so does the drag. Focus follows the
        // pointer from there (`capture_surface_focus`), so the monitor
        // being worked on is the one that hears the keyboard.
        .focused(index == 0)
        .build()
        .map_err(|e| e.to_string())
        .map(|window| {
            // Position and size in physical pixels, which is what the
            // monitor was measured in — a logical size would be short by
            // the scale factor on a high-DPI display.
            let _ = window.set_position(PhysicalPosition::new(screen.bounds.x, screen.bounds.y));
            let _ = window.set_size(PhysicalSize::new(screen.bounds.width, screen.bounds.height));
        });
        let _ = done.send(result);
    })
    .map_err(|e| e.to_string())?;

    built
        .await
        .map_err(|_| "the capture surface never opened".to_string())?
}

/// Show a surface, now that it has something to show.
pub fn reveal(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        if index_of(label) == Some(0) {
            let _ = window.set_focus();
        }
    }
}

/// Give the keyboard to the surface the pointer is over.
///
/// With one window per monitor, Escape would otherwise only work on
/// whichever surface happened to open focused — the user would move to
/// the other screen to select, press Escape, and nothing would happen.
pub fn focus(app: &AppHandle, label: &str) {
    if !is_running() {
        return;
    }
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply from a window whose session already ended must not resolve
    /// the next one — the surfaces are closed by the session, and a stray
    /// `finish` is exactly what a late event looks like.
    #[test]
    fn finishing_without_a_session_is_harmless() {
        finish(&label(0), Some(Fractions { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }));
        finish(&label(3), None);
        assert!(!is_running());
        assert_eq!(surface(&label(0)).image, "");
    }

    /// The label is the only thing a reply carries about which monitor it
    /// came from, and it is what gates the commands, so it has to round
    /// trip exactly — and nothing else may look like one.
    #[test]
    fn a_surface_label_names_its_monitor() {
        for index in [0, 1, 7, 42] {
            assert_eq!(index_of(&label(index)), Some(index));
            assert!(is_surface(&label(index)));
        }
        assert_eq!(index_of("main"), None);
        assert_eq!(index_of("capture-recorder"), None);
        assert_eq!(index_of("capture-region-"), None);
        assert_eq!(index_of("capture-region-x"), None);
        assert!(!is_surface("tool-0000000000000000"));
    }
}
