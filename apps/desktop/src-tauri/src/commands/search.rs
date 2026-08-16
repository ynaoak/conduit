use tauri::State;

use crate::plugin_system::registry::PluginRegistry;
use crate::plugin_system::types::SearchResult;

#[tauri::command]
pub async fn query(
    query: String,
    registry: State<'_, PluginRegistry>,
) -> Result<Vec<SearchResult>, String> {
    Ok(registry.search(&query).await)
}

/// Enumerate a plugin's registered entries (browsing UI, not search)
#[tauri::command]
pub async fn browse_plugin(
    plugin_id: String,
    registry: State<'_, PluginRegistry>,
) -> Result<Vec<SearchResult>, String> {
    Ok(registry.browse(&plugin_id).await)
}
