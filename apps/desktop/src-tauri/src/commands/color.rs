//! Picking a color, from the launcher or from the Color Palette window.

use tauri::{AppHandle, State, Window};

use crate::color::Rgb;
use crate::color_history::ColorHistoryState;
use crate::screen_color::{self, Outcome};

/// The built-in package, and the apps in it allowed to start a pick: the
/// palette, and the color system, which is seeded by a color off the
/// screen as often as by one typed in.
const PALETTE_WORKFLOW: &str = "conduit.devtools";
const PALETTE_APPS: &[&str] = &["color.html", "theme.html"];

/// Whether a window may start a pick.
///
/// App commands are not gated by the capability files — those cover plugin
/// commands — so any tool window can reach this one, imported packages
/// included. Reading the screen is not something a third-party package
/// should be handed, so the gate is here: the launcher itself, and the one
/// built-in window whose whole purpose this is.
fn may_pick(label: &str) -> bool {
    label == "main"
        || PALETTE_APPS.iter().any(|html| {
            label == crate::plugins::workflows::tool_window::window_label(PALETTE_WORKFLOW, html)
        })
}

/// Run one pick. Returns the color as `#rrggbb`, or `None` if the user
/// cancelled — which is an ordinary way to end, not an error.
#[tauri::command]
pub async fn pick_screen_color(
    window: Window,
    app: AppHandle,
    history: State<'_, ColorHistoryState>,
) -> Result<Option<String>, String> {
    if !may_pick(window.label()) {
        return Err("this window may not read the screen".into());
    }
    if !screen_color::is_supported() {
        return Err(crate::i18n::t("errors", "color_unsupported").to_string());
    }

    // Only the launcher is in the way of what is being sampled; the palette
    // window is where the user just clicked, and hiding it would take the
    // results away with it.
    let hide_first = window.label() == "main";
    Ok(run_pick(&app, &history, hide_first).await?.map(|color| color.hex()))
}

/// One pick session, shared by the command and the launcher plugin so the
/// two cannot drift apart on what a pick does.
pub async fn run_pick(
    app: &AppHandle,
    history: &ColorHistoryState,
    hide_launcher: bool,
) -> Result<Option<Rgb>, String> {
    if hide_launcher {
        crate::window::hide_launcher(app);
    }

    // Aiming at a single pixel needs magnification, on the platforms that
    // do not already provide it.
    let loupe = crate::loupe::Session::start(app);
    let outcome = screen_color::pick(app).await;
    loupe.stop();

    match outcome {
        Ok(Outcome::Picked(color)) => {
            history.remember(color);
            Ok(Some(color))
        }
        Ok(Outcome::Cancelled) => Ok(None),
        Err(message) => Err(message),
    }
}

/// The history as `#rrggbb` strings, newest first. The palette window reads
/// its initial list from an injected snapshot; this is how it refreshes
/// after a pick without reopening.
#[tauri::command]
pub async fn picked_colors(
    window: Window,
    history: State<'_, ColorHistoryState>,
) -> Result<Vec<String>, String> {
    if !may_pick(window.label()) {
        return Err("this window may not read the color history".into());
    }
    Ok(history.snapshot().iter().map(Rgb::hex).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate is the only thing standing between an imported package and
    /// the screen, since app commands are reachable from any window.
    #[test]
    fn only_the_launcher_and_the_palette_may_pick() {
        assert!(may_pick("main"));
        for html in PALETTE_APPS {
            assert!(may_pick(&crate::plugins::workflows::tool_window::window_label(
                PALETTE_WORKFLOW,
                html
            )));
        }

        assert!(!may_pick("tool-0000000000000000"));
        assert!(!may_pick(&crate::plugins::workflows::tool_window::window_label(
            "imported.package",
            "color.html"
        )));
        // A package of its own is not the built-in one, whatever it calls
        // its files
        assert!(!may_pick(&crate::plugins::workflows::tool_window::window_label(
            "imported.package",
            "theme.html"
        )));
        // Another built-in app is still not this one
        assert!(!may_pick(&crate::plugins::workflows::tool_window::window_label(
            PALETTE_WORKFLOW,
            "base64.html"
        )));
        assert!(!may_pick("color-loupe"));
    }
}
