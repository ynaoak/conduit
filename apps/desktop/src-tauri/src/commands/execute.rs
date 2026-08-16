use tauri::State;

use crate::plugin_system::registry::PluginRegistry;

#[tauri::command]
pub async fn execute_action(
    plugin_id: String,
    result_id: String,
    action_id: Option<String>,
    registry: State<'_, PluginRegistry>,
) -> Result<(), String> {
    registry
        .execute(
            &plugin_id,
            &result_id,
            action_id.as_deref().unwrap_or("default"),
        )
        .await
        .map_err(|e| e.to_string())
}
