use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::config::WindowConfig;

/// Pin state: while pinned the window ignores hide-on-blur and stays open.
#[derive(Clone, Default)]
pub struct PinState(Arc<AtomicBool>);

impl PinState {
    pub fn set(&self, pinned: bool) {
        self.0.store(pinned, Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// One-shot blur suppression: arm() before deliberately handing focus to
/// another app (e.g. opening config.json in an editor) so the very next
/// blur does not hide the launcher. Subsequent blurs behave normally.
#[derive(Clone, Default)]
pub struct BlurSuppression(Arc<AtomicBool>);

impl BlurSuppression {
    pub fn arm(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Returns true (and disarms) if the next hide should be skipped
    pub fn consume(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

/// Toggle the launcher window: hide if visible, otherwise show and focus.
/// Shared by the combo hotkey and the double-tap hook.
pub fn toggle_launcher(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.center();
            let _ = window.show();
            let _ = window.set_focus();
            force_keyboard_focus(&window);
            let _ = window.emit("conduit://focus-search", ());
        }
    }
}

/// Windows' foreground lock keeps focus in the current app when a process
/// that did not receive the last input shows a window — exactly our situation
/// when summoned from the keyboard hook (the double-tapped Ctrl went to the
/// foreground app, not to us). Attach to the foreground thread's input queue
/// so SetForegroundWindow succeeds and typing lands in the launcher at once.
#[cfg(windows)]
fn force_keyboard_focus(window: &tauri::WebviewWindow) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == hwnd {
            return;
        }
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let this_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != this_thread
            && AttachThreadInput(this_thread, foreground_thread, true).as_bool();

        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);

        if attached {
            let _ = AttachThreadInput(this_thread, foreground_thread, false);
        }
    }
}

#[cfg(not(windows))]
fn force_keyboard_focus(_window: &tauri::WebviewWindow) {}

pub fn setup_main_window(app: &AppHandle, window_config: &WindowConfig) -> anyhow::Result<()> {
    let window = app
        .get_webview_window("main")
        .expect("main window not found");

    let pin_state = PinState::default();
    app.manage(pin_state.clone());
    let blur_suppression = BlurSuppression::default();
    app.manage(blur_suppression.clone());

    if window_config.hide_on_blur {
        let win_clone = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(false) = event {
                // Pinned windows stay open when focus moves elsewhere.
                // A blur while our own process is still the foreground window
                // is internal (e.g. the drag move-loop taking focus from the
                // webview) — hiding then would abort dragging, so skip it.
                if !pin_state.get()
                    && !foreground_is_own_process()
                    && !blur_suppression.consume()
                {
                    let _ = win_clone.hide();
                }
            }
        });
    }

    Ok(())
}

/// True when the current foreground window belongs to this process
#[cfg(windows)]
fn foreground_is_own_process() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return false;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        pid == std::process::id()
    }
}

#[cfg(not(windows))]
fn foreground_is_own_process() -> bool {
    false
}
