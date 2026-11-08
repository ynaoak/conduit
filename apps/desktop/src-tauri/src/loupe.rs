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
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

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
/// The magnifier has to keep up with a hand moving a mouse; anything
/// slower reads as "it does not follow". Affordable now that a frame is
/// one blit rather than two hundred screen reads.
const FRAME: Duration = Duration::from_millis(16);

#[derive(Serialize, Clone)]
struct Frame {
    /// Row-major, `GRID * GRID` pixels, six hex digits each and no
    /// separators. One string rather than an array of them: this crosses
    /// the IPC sixty times a second, and 225 short JSON strings per frame
    /// is most of the work in sending it.
    pixels: String,
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
    ///
    /// A failure to open it is not a reason to abandon the pick: clicking
    /// still takes a color, it is just harder to aim. It *is* worth
    /// reporting, though — a pick with no magnifier and no explanation is
    /// what a silently dropped window looks like from the outside.
    pub async fn start(app: &AppHandle) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        let session = Self {
            app: app.clone(),
            running: running.clone(),
        };

        if screen_color::platform_draws_the_loupe() || !screen_color::is_supported() {
            return session;
        }

        if let Err(message) = open_window(app).await {
            eprintln!("color loupe unavailable: {message}");
            return session;
        }

        running.store(true, Ordering::SeqCst);
        let app = app.clone();
        // A plain OS thread, not the async runtime: reading the screen is
        // blocking work that has no business occupying a tokio worker, and
        // the loop is a fixed tick rather than anything that awaits.
        std::thread::spawn(move || {
            let mut previous: Option<(i32, i32, String)> = None;
            while running.load(Ordering::SeqCst) {
                if let Some((x, y)) = screen_color::cursor_position() {
                    let pixels = screen_color::capture_square(x, y, GRID);
                    if !pixels.is_empty() {
                        let payload = frame(&pixels);
                        // Skip the move and the IPC when nothing about the
                        // frame differs — a still pointer over still pixels
                        // should cost nothing.
                        let unchanged = previous
                            .as_ref()
                            .is_some_and(|(px, py, pixels)| *px == x && *py == y && *pixels == payload.pixels);
                        if !unchanged {
                            if let Some(window) = app.get_webview_window(LABEL) {
                                let _ = window
                                    .set_position(PhysicalPosition::new(x + OFFSET, y + OFFSET));
                                let _ = window.emit_to(LABEL, "conduit://loupe", payload.clone());
                            }
                            previous = Some((x, y, payload.pixels));
                        }
                    }
                }
                std::thread::sleep(FRAME);
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

/// Create the magnifier window.
///
/// On the main thread, like every other window in this app: creating one
/// from the async runtime is not supported everywhere, and the failure is
/// a window that never appears rather than an error anyone sees.
///
/// Shown as soon as it exists rather than waiting for the first frame. The
/// wait was there to avoid a flash of empty glass, but it also meant that
/// anything wrong further down the pipeline showed up as nothing at all.
async fn open_window(app: &AppHandle) -> Result<(), String> {
    let (done, built) = oneshot::channel();
    let handle = app.clone();

    app.run_on_main_thread(move || {
        // `focused(false)` is the load-bearing one: a magnifier that takes
        // focus would change what the user is pointing at.
        let result = WebviewWindowBuilder::new(
            &handle,
            LABEL,
            WebviewUrl::App("index.html#loupe".into()),
        )
        .title("Conduit color loupe")
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .build()
        .map_err(|e| e.to_string())
        .map(|window| {
            // The click has to reach whatever is underneath, not the loupe
            let _ = window.set_ignore_cursor_events(true);
        });
        let _ = done.send(result);
    })
    .map_err(|e| e.to_string())?;

    built
        .await
        .map_err(|_| "the color loupe never opened".to_string())?
}

/// Build the payload for one frame. Separate from the loop so the shape of
/// what the window receives can be checked without a screen.
fn frame(pixels: &[Rgb]) -> Frame {
    let center = pixels
        .get(pixels.len() / 2)
        .copied()
        .unwrap_or(Rgb::new(0, 0, 0));
    let mut packed = String::with_capacity(pixels.len() * 6);
    for pixel in pixels {
        packed.push_str(&format!("{:02x}{:02x}{:02x}", pixel.r, pixel.g, pixel.b));
    }
    Frame {
        pixels: packed,
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
        // Six hex digits per pixel, no separators — the window slices it
        assert_eq!(frame.pixels.len(), (GRID * GRID) as usize * 6);
        assert_eq!(&frame.pixels[middle * 6..middle * 6 + 6], "ff8800");
        assert!(frame.pixels.starts_with("000000"), "the rest stays black");
    }

    /// An odd grid is what gives the square a middle at all.
    #[test]
    fn the_grid_has_a_middle_pixel() {
        assert_eq!(GRID % 2, 1, "an even grid has no center pixel to aim with");
    }
}
