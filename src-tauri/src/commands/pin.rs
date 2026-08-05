use tauri::State;

use crate::window::PinState;

/// Toggle the always-open pin: while pinned the window ignores hide-on-blur.
#[tauri::command]
pub async fn set_pinned(pinned: bool, state: State<'_, PinState>) -> Result<(), String> {
    state.set(pinned);
    Ok(())
}
