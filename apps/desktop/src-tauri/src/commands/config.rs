use tauri::State;

use crate::config::{AppConfig, ConfigState};

#[tauri::command]
pub async fn get_config(state: State<'_, ConfigState>) -> Result<AppConfig, String> {
    Ok(state.get().await)
}

#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    state: State<'_, ConfigState>,
) -> Result<(), String> {
    // Plugins and the tray read the language from process state, so a
    // saved change has to reach it before the next search runs
    crate::i18n::set(&config.language);
    state.update(config).await.map_err(|e| e.to_string())
}

/// The language the backend is actually rendering in.
///
/// The frontend can resolve `language` itself for an explicit code, but not
/// for "system": it would have to ask the webview (`navigator.languages`)
/// while plugin results come from `i18n::t`, which follows the OS locale.
/// Those two can disagree, which would mix languages inside one window — so
/// the frontend mirrors this instead of deciding for itself.
#[tauri::command]
pub async fn resolved_language() -> Result<String, String> {
    Ok(crate::i18n::current().to_string())
}

/// Which channel this build came from, so the UI can say so and skip the
/// update controls where they would not work.
#[tauri::command]
pub async fn release_channel() -> Result<String, String> {
    Ok(crate::channel::CHANNEL.to_string())
}

/// The chord that summons the launcher, named the way this platform names
/// its keys. The About tab shows it because on any platform without the
/// double-tap detector there is otherwise nothing on screen that says how to
/// get the window back once it hides.
#[tauri::command]
pub async fn summon_hotkey(state: State<'_, ConfigState>) -> Result<String, String> {
    Ok(crate::config::summon_label(&state.get().await.hotkey))
}

#[tauri::command]
pub async fn get_config_path(state: State<'_, ConfigState>) -> Result<String, String> {
    Ok(state.config_path().display().to_string())
}

/// Restart the app so startup-only settings (hotkey, double_tap) take effect
#[tauri::command]
pub async fn restart_app(app: tauri::AppHandle) -> Result<(), String> {
    app.restart();
}

/// Open config.json in the OS default editor (for restart-required settings)
#[tauri::command]
pub async fn open_config_file(
    state: State<'_, ConfigState>,
    suppression: State<'_, crate::window::BlurSuppression>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    // The editor will steal focus — keep the launcher open through that blur
    suppression.arm();
    let path = state.config_path().display().to_string();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}
