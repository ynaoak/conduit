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
    state.update(config).await.map_err(|e| e.to_string())
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
