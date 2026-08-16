//! The macOS half of a pick session.
//!
//! `NSColorSampler` is the system's own eyedropper — the loupe every native
//! app shows. Using it means the magnifier, the click handling and the
//! escape key all behave exactly as they do elsewhere on the system, and,
//! unlike capturing the screen ourselves, it needs no Screen Recording
//! permission: the user is picking, not the app reading.
//!
//! AppKit documents the handler as running on the main thread, and as being
//! passed nil when the user cancels.

use std::sync::Mutex;

use block2::RcBlock;
use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};
use tauri::AppHandle;
use tokio::sync::oneshot;

use super::Outcome;
use crate::color::Rgb;

/// Show the system sampler and report what the user picked.
///
/// The sampler has to be started from the main thread, and its handler
/// arrives there too — so the result crosses back to the caller through a
/// channel rather than by blocking anything.
pub async fn pick(app: &AppHandle) -> Result<Outcome, String> {
    let (sender, receiver) = oneshot::channel();
    // The handler is an `Fn` block: AppKit could in principle call it more
    // than once, and a oneshot sender can only be used up once. Taking it
    // out of the Option makes a second call a no-op instead of a panic.
    let sender = Mutex::new(Some(sender));

    app.run_on_main_thread(move || {
        let handler = RcBlock::new(move |color: *mut NSColor| {
            let outcome = unsafe { color.as_ref() }
                .and_then(to_rgb)
                .map_or(Outcome::Cancelled, Outcome::Picked);
            if let Some(sender) = sender.lock().ok().and_then(|mut slot| slot.take()) {
                let _ = sender.send(outcome);
            }
        });
        let sampler = NSColorSampler::new();
        unsafe { sampler.showSamplerWithSelectionHandler(&handler) };
    })
    .map_err(|e| format!("could not start the color sampler: {e}"))?;

    receiver
        .await
        .map_err(|_| "the color pick ended without a result".to_string())
}

/// Read a color as sRGB.
///
/// The sampler hands back whatever color space the sampled pixel was in, and
/// its components are meaningless until converted — reading `redComponent`
/// off a CMYK color would give a number that is not red at all.
fn to_rgb(color: &NSColor) -> Option<Rgb> {
    let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let scale = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Rgb::new(
        scale(srgb.redComponent()),
        scale(srgb.greenComponent()),
        scale(srgb.blueComponent()),
    ))
}
