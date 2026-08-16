use tauri::State;

use crate::window::{is_safe_css_name, is_safe_css_value, PinState, RenderedTheme, ResolvedTheme};

/// Toggle the always-open pin: while pinned the launcher never hides on
/// its own (focus loss, or the hide that follows running an action).
#[tauri::command]
pub async fn set_pinned(pinned: bool, state: State<'_, PinState>) -> Result<(), String> {
    state.set(pinned);
    Ok(())
}

/// Hide the launcher (Esc / close button).
/// Goes through the Rust side so the hide also flips the intent state and
/// hands focus back to the previous window — a bare webview hide() leaves
/// the next foreground window to chance.
#[tauri::command]
pub async fn hide_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::window::hide_launcher(&app);
    Ok(())
}

/// Hide after an action ran. Unlike `hide_window` this respects the pin:
/// a pinned launcher stays open so the user can keep working in it.
#[tauri::command]
pub async fn auto_hide_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::window::auto_hide_launcher(&app);
    Ok(())
}

/// Report what the launcher webview is actually rendering: the light/dark
/// verdict and the resolved theme colors. Called by useTheme after it
/// applies mode + overrides, since the overrides can flip the effective
/// look independently of theme.mode — and since tool windows are styled
/// from these values rather than from their own copy of the palette.
#[tauri::command]
pub async fn set_resolved_theme(
    theme: RenderedTheme,
    state: State<'_, ResolvedTheme>,
) -> Result<(), String> {
    if theme.mode != "dark" && theme.mode != "light" {
        return Err(format!("unknown theme: {}", theme.mode));
    }
    // These values end up inside a <style> block in tool windows and
    // config.json can put anything in them, so drop whatever would not be
    // a plain CSS value rather than trusting the round trip.
    let colors = theme
        .colors
        .into_iter()
        .filter(|(name, value)| is_safe_css_name(name) && is_safe_css_value(value))
        .collect();
    state.set(RenderedTheme {
        mode: theme.mode,
        colors,
    });
    Ok(())
}
