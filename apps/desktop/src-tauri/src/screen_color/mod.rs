//! Reading a color off the screen, wherever the pointer happens to be.
//!
//! Each platform gets there differently, and the difference is not an
//! implementation detail — it decides what the UI has to provide:
//!
//! * **Windows** has no system color picker, so the session is ours: a
//!   low-level mouse hook takes the next click, and the magnifier that
//!   makes single-pixel aiming possible is a window we draw ourselves.
//! * **macOS** has `NSColorSampler`, the same loupe every native app uses.
//!   It runs the whole session — magnifier, click, escape — and hands back
//!   a color. Drawing our own on top of it would be the wrong thing.
//!
//! So `pick` is the shared entry point, and `has_loupe` tells the caller
//! whether it still has to put a magnifier on screen.

use tauri::AppHandle;

use crate::color::Rgb;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Picked(Rgb),
    /// Escape, a right-click, or the system sampler's own cancel.
    Cancelled,
}

/// True when the platform runs its own magnifier, so the caller should not
/// open one of its own.
pub const fn platform_draws_the_loupe() -> bool {
    cfg!(target_os = "macos")
}

/// True when picking is implemented at all here. Checked before a result is
/// offered, so an unsupported platform never shows an action that can only
/// report failure.
pub const fn is_supported() -> bool {
    cfg!(windows) || cfg!(target_os = "macos")
}

/// Run one interactive pick. Resolves when the user has clicked a pixel or
/// cancelled; the launcher window is expected to be hidden by then, since
/// the point is to sample what is *behind* it.
pub async fn pick(app: &AppHandle) -> Result<Outcome, String> {
    #[cfg(windows)]
    {
        let _ = app;
        return windows::pick().await;
    }
    #[cfg(target_os = "macos")]
    {
        return macos::pick(app).await;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = app;
        Err("screen color picking is not implemented on this platform".into())
    }
}

/// A square of pixels centred on a point, row-major, for the magnifier.
/// Empty where the platform draws its own loupe (or has no implementation),
/// which is exactly when the caller does not need it.
#[allow(unused_variables)]
pub fn capture_square(center_x: i32, center_y: i32, size: u32) -> Vec<Rgb> {
    #[cfg(windows)]
    {
        windows::capture_square(center_x, center_y, size)
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Where the pointer is, in screen coordinates.
pub fn cursor_position() -> Option<(i32, i32)> {
    #[cfg(windows)]
    {
        windows::cursor_position()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two flags have to disagree on macOS: the system sampler draws the
    /// loupe, so opening ours as well would put two magnifiers on screen.
    #[test]
    fn only_the_platform_without_a_sampler_draws_its_own_loupe() {
        if cfg!(target_os = "macos") {
            assert!(is_supported());
            assert!(platform_draws_the_loupe());
        } else if cfg!(windows) {
            assert!(is_supported());
            assert!(!platform_draws_the_loupe());
        } else {
            assert!(!is_supported());
        }
    }
}
