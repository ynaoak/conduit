use tauri::State;

use crate::pins::{PinnedItem, PinsState};

#[tauri::command]
pub async fn list_pins(state: State<'_, PinsState>) -> Result<Vec<PinnedItem>, String> {
    Ok(state.list().await)
}

#[tauri::command]
pub async fn add_pin(item: PinnedItem, state: State<'_, PinsState>) -> Result<(), String> {
    state.add(item).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_pin(result_id: String, state: State<'_, PinsState>) -> Result<(), String> {
    state.remove(&result_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn move_pin(
    result_id: String,
    new_index: usize,
    state: State<'_, PinsState>,
) -> Result<(), String> {
    state
        .move_to(&result_id, new_index)
        .await
        .map_err(|e| e.to_string())
}
