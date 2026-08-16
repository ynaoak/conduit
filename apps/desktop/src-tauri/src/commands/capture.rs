//! Taking a screenshot or a recording, and living with the result.
//!
//! One flow, reachable from three places: a launcher row, the gallery
//! window, and the selection surface answering a drag. `take` is the flow
//! itself, so the row and the window cannot drift apart on what "capture
//! a region" means.
//!
//! Like the color commands, these are app commands, which the capability
//! files do not gate — so the gate is `may_capture`, by window label.
//! Reading the screen and reaching the captures already taken is not
//! something an imported package gets handed.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, State, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::capture::{self, record, region, Fractions, Screen};
use crate::capture_history::{now_ms, Capture, CaptureStore, GalleryEntry};

/// The built-in package and app that may drive captures: the gallery.
const GALLERY_WORKFLOW: &str = "conduit.devtools";
const GALLERY_HTML: &str = "capture.html";

/// How long to wait after hiding the launcher before freezing the screen.
///
/// Hiding a window is a request to the compositor, not an event that has
/// already happened; capturing immediately catches the launcher still on
/// screen, in its own screenshot.
const HIDE_SETTLE: Duration = Duration::from_millis(140);

fn gallery_label() -> String {
    crate::plugins::workflows::tool_window::window_label(GALLERY_WORKFLOW, GALLERY_HTML)
}

/// Whether a window may take captures and see the ones already taken.
fn may_capture(label: &str) -> bool {
    label == "main" || label == gallery_label()
}

/// What a capture is: the two stills and the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Drag a rectangle over a frozen screen
    Region,
    /// The whole monitor the pointer is on
    Screen,
    /// Drag a rectangle, then record it
    Record,
}

#[tauri::command]
pub async fn take_capture(
    window: Window,
    app: AppHandle,
    store: State<'_, CaptureStore>,
    mode: Mode,
    screen: Option<usize>,
) -> Result<Option<Capture>, String> {
    if !may_capture(window.label()) {
        return Err("this window may not capture the screen".into());
    }
    // Only the launcher is in the way of what is being captured. The
    // gallery is where the user just clicked, and hiding it would take the
    // history away with the click.
    take(&app, &store, mode, screen, window.label() == "main").await
}

/// One capture, from whichever surface asked for it.
///
/// `screen` names a monitor for a full-screen shot; without it, a
/// full-screen shot is every monitor composited into one image. A region
/// or a recording ignores it — the monitor is whichever surface the user
/// drags on.
///
/// `Ok(None)` means the user cancelled the selection, or that a recording
/// started — a recording files itself when it stops, not here.
pub async fn take(
    app: &AppHandle,
    store: &CaptureStore,
    mode: Mode,
    screen: Option<usize>,
    hide_launcher: bool,
) -> Result<Option<Capture>, String> {
    if !capture::is_supported() {
        return Err(crate::i18n::t("errors", "capture_unsupported").to_string());
    }
    if region::is_running() {
        return Err(crate::i18n::t("errors", "capture_busy").to_string());
    }
    if matches!(mode, Mode::Record) && record::is_running() {
        return Err(crate::i18n::t("errors", "record_busy").to_string());
    }

    if hide_launcher {
        crate::window::hide_launcher(app);
        tokio::time::sleep(HIDE_SETTLE).await;
    }

    // A full-screen shot of one named monitor is the only case that does
    // not need every display frozen.
    if let (Mode::Screen, Some(index)) = (mode, screen) {
        let (_, shot) = capture::capture_one(app, index)?;
        return store.add_image(&shot, now_ms()).map(Some);
    }

    let frames = capture::capture_all(app)?;

    if matches!(mode, Mode::Screen) {
        let shot = capture::composite(&frames)
            .ok_or_else(|| crate::i18n::t("errors", "capture_failed").to_string())?;
        return store.add_image(&shot, now_ms()).map(Some);
    }

    let Some(selection) = region::select(app, &frames, matches!(mode, Mode::Record)).await? else {
        // Cancelled: not an error, and nothing to report
        return Ok(None);
    };

    let frozen = &frames
        .get(selection.screen.index)
        .ok_or_else(|| crate::i18n::t("errors", "capture_failed").to_string())?
        .1;
    let shot = frozen
        .crop(selection.in_frame)
        .ok_or_else(|| crate::i18n::t("errors", "capture_failed").to_string())?;

    if matches!(mode, Mode::Record) {
        record::start(app, store, selection.on_screen, selection.screen, shot).await?;
        return Ok(None);
    }

    store.add_image(&shot, now_ms()).map(Some)
}

/// Every monitor, for a window offering a choice between them.
#[tauri::command]
pub fn capture_screens(window: Window, app: AppHandle) -> Result<Vec<Screen>, String> {
    if !may_capture(window.label()) {
        return Err("this window may not read the screen".into());
    }
    Ok(capture::screens(&app))
}

/// The frozen screen the selection surface draws. Asked for by the
/// surface once it has mounted, rather than pushed to it — a window that
/// misses the event it was opened for would show a blank rectangle over
/// the screen and take a drag on it.
#[tauri::command]
pub fn capture_frame(window: Window) -> Result<region::Surface, String> {
    if !region::is_surface(window.label()) {
        return Err("this window may not read the screen".into());
    }
    Ok(region::surface(window.label()))
}

/// The surface has the frame painted and can be shown.
#[tauri::command]
pub fn capture_surface_ready(window: Window, app: AppHandle) -> Result<(), String> {
    if !region::is_surface(window.label()) {
        return Err("this window may not read the screen".into());
    }
    region::reveal(&app, window.label());
    Ok(())
}

/// The pointer moved onto this surface, so the keyboard should follow it.
/// Without this, Escape only works on the monitor whose surface opened
/// focused — and the user is by definition on another one.
#[tauri::command]
pub fn capture_surface_focus(window: Window, app: AppHandle) -> Result<(), String> {
    if !region::is_surface(window.label()) {
        return Err("this window may not take focus".into());
    }
    region::focus(&app, window.label());
    Ok(())
}

/// The drag ended. `None` is Escape or a right-click.
#[tauri::command]
pub fn capture_selected(window: Window, selection: Option<Fractions>) -> Result<(), String> {
    if !region::is_surface(window.label()) {
        return Err("this window may not capture the screen".into());
    }
    region::finish(window.label(), selection);
    Ok(())
}

/// Stop the recording. Reachable from the recorder's own window as well
/// as the launcher and the gallery, since that window is the stop button.
#[tauri::command]
pub async fn stop_recording(window: Window) -> Result<Capture, String> {
    if !may_capture(window.label()) && window.label() != record::LABEL {
        return Err("this window may not stop the recording".into());
    }
    record::stop().await
}

/// Whether something is being recorded, for a window that opened while it
/// was already running.
#[tauri::command]
pub fn recording_state() -> RecordingState {
    RecordingState {
        running: record::is_running(),
        max_seconds: record::MAX_SECONDS,
    }
}

#[derive(Serialize)]
pub struct RecordingState {
    pub running: bool,
    pub max_seconds: u64,
}

/// The history, previews included — what the gallery redraws itself from
/// after a capture, a delete or a recording.
#[tauri::command]
pub fn capture_list(
    window: Window,
    store: State<'_, CaptureStore>,
) -> Result<Vec<GalleryEntry>, String> {
    if !may_capture(window.label()) {
        return Err("this window may not read the captures".into());
    }
    Ok(store.gallery())
}

/// Put a capture on the clipboard: the image itself for a still, its path
/// for a recording — no system clipboard takes a movie, and a path is
/// what a person pastes into the app that does.
#[tauri::command]
pub fn copy_capture(
    window: Window,
    app: AppHandle,
    store: State<'_, CaptureStore>,
    id: String,
) -> Result<(), String> {
    if !may_capture(window.label()) {
        return Err("this window may not read the captures".into());
    }
    let capture = found(&store, &id)?;
    copy(&app, &store, &capture)
}

pub fn copy(app: &AppHandle, store: &CaptureStore, capture: &Capture) -> Result<(), String> {
    let path = store.path(&capture.file);
    if capture.kind.is_video() {
        return app
            .clipboard()
            .write_text(path.to_string_lossy().to_string())
            .map_err(|e| format!("failed to copy to clipboard: {}", e));
    }

    let bytes = std::fs::read(&path).map_err(|e| format!("failed to read the capture: {}", e))?;
    let shot = capture::decode_png(&bytes)?;
    app.clipboard()
        .write_image(&tauri::image::Image::new(&shot.rgba, shot.width, shot.height))
        .map_err(|e| format!("failed to copy to clipboard: {}", e))
}

#[tauri::command]
pub fn open_capture(
    window: Window,
    app: AppHandle,
    store: State<'_, CaptureStore>,
    id: String,
) -> Result<(), String> {
    if !may_capture(window.label()) {
        return Err("this window may not read the captures".into());
    }
    let capture = found(&store, &id)?;
    app.opener()
        .open_path(store.path(&capture.file).to_string_lossy(), None::<&str>)
        .map_err(|e| format!("failed to open the capture: {}", e))
}

#[tauri::command]
pub fn reveal_capture(
    window: Window,
    app: AppHandle,
    store: State<'_, CaptureStore>,
    id: String,
) -> Result<(), String> {
    if !may_capture(window.label()) {
        return Err("this window may not read the captures".into());
    }
    let capture = found(&store, &id)?;
    app.opener()
        .reveal_item_in_dir(store.path(&capture.file))
        .map_err(|e| format!("failed to open the folder: {}", e))
}

#[tauri::command]
pub fn delete_capture(
    window: Window,
    store: State<'_, CaptureStore>,
    id: String,
) -> Result<(), String> {
    if !may_capture(window.label()) {
        return Err("this window may not change the captures".into());
    }
    if !store.remove(&id) {
        return Err(crate::i18n::t("errors", "capture_missing").to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn clear_captures(window: Window, store: State<'_, CaptureStore>) -> Result<(), String> {
    if !may_capture(window.label()) {
        return Err("this window may not change the captures".into());
    }
    store.clear();
    Ok(())
}

/// Where the captures are kept, for the gallery's "show me the folder".
#[tauri::command]
pub fn open_captures_folder(
    window: Window,
    app: AppHandle,
    store: State<'_, CaptureStore>,
) -> Result<(), String> {
    if !may_capture(window.label()) {
        return Err("this window may not read the captures".into());
    }
    app.opener()
        .open_path(store.dir().to_string_lossy(), None::<&str>)
        .map_err(|e| format!("failed to open the folder: {}", e))
}

fn found(store: &CaptureStore, id: &str) -> Result<Capture, String> {
    store
        .get(id)
        .ok_or_else(|| crate::i18n::t("errors", "capture_missing").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// App commands are reachable from any window, so this gate is the
    /// only thing between an imported package and both the screen and
    /// every capture already taken.
    #[test]
    fn only_the_launcher_and_the_gallery_may_capture() {
        assert!(may_capture("main"));
        assert!(may_capture(&gallery_label()));

        assert!(!may_capture("tool-0000000000000000"));
        assert!(!may_capture(&crate::plugins::workflows::tool_window::window_label(
            "imported.package",
            "capture.html"
        )));
        // Another built-in app is still not this one
        assert!(!may_capture(&crate::plugins::workflows::tool_window::window_label(
            GALLERY_WORKFLOW,
            "color.html"
        )));
        // The surfaces and the recorder answer to their own commands only
        assert!(!may_capture(&region::label(0)));
        assert!(!may_capture(&region::label(1)));
        assert!(!may_capture(record::LABEL));
    }
}
