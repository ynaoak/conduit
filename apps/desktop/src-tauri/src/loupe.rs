//! The magnifier shown while picking a color on Windows.
//!
//! macOS needs none of this — `NSColorSampler` brings its own loupe, and a
//! second one would be both wrong and in the way. So everything here is
//! inert unless the platform leaves the magnifying to us.
//!
//! The window is a plain webview like any other; what makes it usable as a
//! magnifier is that it never takes focus and ignores the pointer, so the
//! click that ends the session goes to the screen underneath it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::color::Rgb;
use crate::screen_color;

pub const LABEL: &str = "color-loupe";
/// Pixels sampled across. Odd, so there is a middle one to aim with.
const GRID: u32 = 15;
/// On-screen width of the window, in logical pixels. The glass is square,
/// so the height adds the label strip: 4px padding, 24px label, 4px gap.
const WINDOW_WIDTH: f64 = 168.0;
const WINDOW_HEIGHT: f64 = WINDOW_WIDTH + 32.0;
/// How far from the pointer the window sits, so it never covers the pixel
/// being aimed at.
const OFFSET: i32 = 24;
/// ~20 frames a second: smooth enough to aim with, and far cheaper than
/// redrawing on every mouse move.
const FRAME: Duration = Duration::from_millis(50);

#[derive(Serialize, Clone)]
struct Frame {
    /// Row-major, `GRID * GRID` entries, each `#rrggbb`
    pixels: Vec<String>,
    grid: u32,
    /// The color under the pointer — the one a click would take
    center: String,
    /// Black or white, whichever stays readable on `center`. Decided here
    /// so the window does not need a second copy of the contrast rule.
    ink: String,
}

/// A magnifier for the duration of one pick.
pub struct Session {
    app: AppHandle,
    running: Arc<AtomicBool>,
}

impl Session {
    /// Open the magnifier, if this platform wants one.
    pub fn start(app: &AppHandle) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        let session = Self {
            app: app.clone(),
            running: running.clone(),
        };

        if screen_color::platform_draws_the_loupe() || !screen_color::is_supported() {
            return session;
        }

        // `focused(false)` is the load-bearing one: a magnifier that takes
        // focus would hide the launcher's own window state and, worse,
        // change what the user is pointing at.
        let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#loupe".into()))
            .title("Conduit color loupe")
            .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .focused(false)
            .visible(false)
            .build();

        let Ok(window) = window else {
            return session;
        };
        // The click has to reach whatever is underneath, not the loupe
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.set_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        running.store(true, Ordering::SeqCst);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut shown = false;
            while running.load(Ordering::SeqCst) {
                if let Some((x, y)) = screen_color::cursor_position() {
                    let pixels = screen_color::capture_square(x, y, GRID);
                    if !pixels.is_empty() {
                        if let Some(window) = app.get_webview_window(LABEL) {
                            let _ = window.set_position(PhysicalPosition::new(x + OFFSET, y + OFFSET));
                            let _ = window.emit_to(LABEL, "conduit://loupe", frame(&pixels));
                            // Shown only once there is something to show, so
                            // the first paint is never an empty rectangle
                            if !shown {
                                let _ = window.show();
                                shown = true;
                            }
                        }
                    }
                }
                tokio::time::sleep(FRAME).await;
            }
            if let Some(window) = app.get_webview_window(LABEL) {
                let _ = window.close();
            }
        });

        session
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        // Closing here as well as in the task: the task waits up to a frame
        // before noticing, and the magnifier should go the moment the pick
        // does.
        if let Some(window) = self.app.get_webview_window(LABEL) {
            let _ = window.close();
        }
    }
}

/// Build the payload for one frame. Separate from the loop so the shape of
/// what the window receives can be checked without a screen.
fn frame(pixels: &[Rgb]) -> Frame {
    let center = pixels
        .get(pixels.len() / 2)
        .copied()
        .unwrap_or(Rgb::new(0, 0, 0));
    Frame {
        pixels: pixels.iter().map(Rgb::hex).collect(),
        grid: GRID,
        center: center.hex(),
        ink: center.contrasting_ink().hex(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The center cell is what the crosshair sits on and what a click would
    /// take, so an off-by-one here would report the wrong color while
    /// looking perfectly plausible.
    #[test]
    fn the_center_cell_is_the_middle_of_the_square() {
        let mut pixels = vec![Rgb::new(0, 0, 0); (GRID * GRID) as usize];
        let middle = (GRID * GRID / 2) as usize;
        pixels[middle] = Rgb::new(0xff, 0x88, 0x00);

        let frame = frame(&pixels);
        assert_eq!(frame.center, "#ff8800");
        assert_eq!(frame.ink, "#000000", "the label has to stay readable on the swatch");
        assert_eq!(frame.pixels.len(), (GRID * GRID) as usize);
        assert_eq!(frame.pixels[middle], "#ff8800");
    }

    /// An odd grid is what gives the square a middle at all.
    #[test]
    fn the_grid_has_a_middle_pixel() {
        assert_eq!(GRID % 2, 1, "an even grid has no center pixel to aim with");
    }
}
