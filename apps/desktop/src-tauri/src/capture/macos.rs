//! Capturing pixels on macOS, through `screencapture`.
//!
//! The system tool rather than `CGDisplayCreateImage` bindings, for the
//! same reason the color picker uses `NSColorSampler`: this is the path
//! the OS already knows how to ask permission for. Screen Recording is a
//! consent the user grants to the app once, in System Settings, and the
//! prompt that starts that flow is the system's to show.
//!
//! `-R` is addressed in points while the file comes back in pixels, so a
//! Retina display answers a 400-point request with an 800-pixel image.
//! That is the reason `capture` takes the scale factor: the caller asked
//! for physical pixels and has to be given them.

use std::process::Command;

use super::{decode_png, Rect, Shot};

const SCREENCAPTURE: &str = "/usr/sbin/screencapture";

pub fn capture(area: Rect, scale: f64) -> Result<Shot, String> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let file = std::env::temp_dir().join(format!(
        "conduit-capture-{}-{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));

    let region = format!(
        "{},{},{},{}",
        area.x as f64 / scale,
        area.y as f64 / scale,
        area.width as f64 / scale,
        area.height as f64 / scale
    );

    // -x: no shutter sound. A screenshot tool that clicks every frame of a
    // recording would be unusable.
    let status = Command::new(SCREENCAPTURE)
        .args(["-x", "-t", "png", "-R", &region])
        .arg(&file)
        .status()
        .map_err(|e| format!("failed to run screencapture: {}", e))?;

    let bytes = std::fs::read(&file);
    let _ = std::fs::remove_file(&file);

    if !status.success() {
        return Err(crate::i18n::t("errors", "capture_failed").to_string());
    }
    decode_png(&bytes.map_err(|e| format!("failed to read the capture: {}", e))?)
}

/// The recorder's own path to `screencapture`, so the video side does not
/// hard-code it a second time.
pub const fn tool() -> &'static str {
    SCREENCAPTURE
}
