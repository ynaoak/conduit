use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};

use crate::config::WindowConfig;

/// The visibility we *intend*, flipped synchronously by every show/hide path.
/// `window.is_visible()` lags the async show/hide calls, so a double-tap
/// arriving right after Esc would read "still visible" and hide again —
/// exactly the "Esc then quick re-summon does nothing" failure.
static INTENDED_VISIBLE: AtomicBool = AtomicBool::new(false);

/// HWND of the window that was foreground when the launcher was summoned
/// (0 = none). Hiding hands focus back to it explicitly instead of letting
/// Windows pick: the OS choice is arbitrary, and if it lands on an elevated
/// window our (unelevated) keyboard hook stops seeing keystrokes, so the
/// Ctrl double-tap can no longer reopen the launcher.
#[cfg_attr(not(windows), allow(dead_code))] // only touched by the win32 paths
static PREV_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

/// When the launcher was last shown (ms since first use). A `Focused(false)`
/// from the previous hide can be delivered *after* a quick re-show; without
/// a grace period that stale blur immediately hides the fresh window.
static SHOWN_AT_MS: AtomicU64 = AtomicU64::new(0);
/// Blur events younger than this after a show are considered stale
const BLUR_GRACE_MS: u64 = 300;

static EPOCH: OnceLock<Instant> = OnceLock::new();

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// What the launcher actually renders.
///
/// It is NOT derivable from `theme.mode` alone: a config that keeps mode
/// "system" while overriding the color slots (every config written before
/// the mode existed does exactly that) renders dark on a light OS. The
/// webview reports what it computed, so tool windows match what the user
/// sees.
///
/// Carries the light/dark verdict plus the resolved values of the theme
/// variables: tool windows are styled from this rather than from a second
/// copy of the palette, so "the same colors as the launcher" stays true
/// even when config.json overrides individual slots.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RenderedTheme {
    /// "dark" or "light"
    pub mode: String,
    /// Tool-window variable name (`surface`, `on-surface`, …) -> CSS value
    #[serde(default)]
    pub colors: std::collections::BTreeMap<String, String>,
}

/// CSS custom-property names are ours, but the values can come from
/// config.json — keep them to things that cannot end a declaration or a
/// style block, so an injected palette stays a palette.
pub fn is_safe_css_value(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 64
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || " #,.%()/-_".contains(c)
        })
}

pub fn is_safe_css_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Clone, Default)]
pub struct ResolvedTheme(Arc<RwLock<Option<RenderedTheme>>>);

impl ResolvedTheme {
    pub fn set(&self, theme: RenderedTheme) {
        *self.0.write().expect("resolved theme lock") = Some(theme);
    }

    pub fn get(&self) -> Option<RenderedTheme> {
        self.0.read().expect("resolved theme lock").clone()
    }
}

/// Pin state: while pinned the launcher never hides on its own. That
/// covers every automatic hide — losing focus *and* the hide that follows
/// launching something — because "stay open" is the whole point of the
/// pin. Esc and the close button are explicit and still hide.
#[derive(Clone, Default)]
pub struct PinState(Arc<AtomicBool>);

impl PinState {
    pub fn set(&self, pinned: bool) {
        self.0.store(pinned, Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Whether an automatic hide may proceed. The single place this policy
    /// is written down; both the blur handler and the post-action hide
    /// ask here.
    pub fn allows_auto_hide(&self) -> bool {
        !self.get()
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
        // Intent AND the real state must agree to count as "visible": intent
        // alone can be stale after a blur-hide, and is_visible() alone lags
        // an in-flight hide. When in doubt, showing is always the safe move.
        let visible =
            INTENDED_VISIBLE.load(Ordering::SeqCst) && window.is_visible().unwrap_or(false);
        if visible {
            // Toggling off is a dismissal, same as Esc
            hide_launcher_window(&window, true);
        } else {
            show_launcher_window(&window);
        }
    }
}

/// Show + focus the launcher, remembering which window held the foreground
/// so hiding can hand it back. All show paths (hotkey, double-tap, tray)
/// must go through here to keep the intent state accurate.
pub fn show_launcher(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        show_launcher_window(&window);
    }
}

/// Hide the launcher, returning focus to the window it was summoned over.
/// Explicit request (Esc, the close button): hides even when pinned, and
/// nothing was launched, so the previous window is the right target.
pub fn hide_launcher(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        hide_launcher_window(&window, true);
    }
}

/// Hide because the launcher is done with its job (an action ran), not
/// because the user asked. A pinned window stays open — opening a tool
/// window or launching an app must not dismiss a launcher the user
/// deliberately pinned.
pub fn auto_hide_launcher(app: &AppHandle) {
    if !app.state::<PinState>().allows_auto_hide() {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        // No hand-back: whatever the action started is the foreground now
        hide_launcher_window(&window, false);
    }
}

fn show_launcher_window(window: &tauri::WebviewWindow) {
    remember_foreground(window);
    INTENDED_VISIBLE.store(true, Ordering::SeqCst);
    SHOWN_AT_MS.store(now_ms(), Ordering::SeqCst);
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    force_keyboard_focus(window);
    let _ = window.emit("conduit://focus-search", ());
}

/// `hand_back` returns the foreground to the window the launcher was
/// summoned over. Right for a dismissal, wrong after running an action:
/// the app (or tool window) that just launched owns the foreground, and
/// yanking it back buries what the user asked for behind their previous
/// window — which reads as "it opened nothing".
fn hide_launcher_window(window: &tauri::WebviewWindow, hand_back: bool) {
    INTENDED_VISIBLE.store(false, Ordering::SeqCst);
    if hand_back {
        restore_foreground(window);
    }
    let _ = window.hide();
}

/// Record the current foreground window unless it is our own (re-summoning
/// while already focused must not overwrite the real "previous" window).
#[cfg(windows)]
fn remember_foreground(window: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return;
        }
        if let Ok(own) = window.hwnd() {
            if foreground == own {
                return;
            }
        }
        PREV_FOREGROUND.store(foreground.0 as isize, Ordering::SeqCst);
    }
}

#[cfg(not(windows))]
fn remember_foreground(_window: &tauri::WebviewWindow) {}

/// Hand the foreground back to the window the launcher was summoned over.
/// As the current foreground process we are allowed to call
/// SetForegroundWindow directly; if the remembered window is gone, fall
/// back to letting Windows pick (the previous behavior).
#[cfg(windows)]
fn restore_foreground(_window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible, SetForegroundWindow};
    let raw = PREV_FOREGROUND.load(Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    unsafe {
        let hwnd = HWND(raw as *mut _);
        if IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn restore_foreground(_window: &tauri::WebviewWindow) {}

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
    app.manage(ResolvedTheme::default());

    if window_config.hide_on_blur {
        let win_clone = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(false) = event {
                // Pinned windows stay open when focus moves elsewhere.
                // A blur while our own process is still the foreground window
                // is internal (e.g. the drag move-loop taking focus from the
                // webview) — hiding then would abort dragging, so skip it.
                // A blur right after a show is stale — the queued Focused(false)
                // from the previous hide arriving late — and must not close
                // the window the user just re-summoned.
                if pin_state.allows_auto_hide()
                    && !foreground_is_own_process()
                    && !blur_suppression.consume()
                    && now_ms().saturating_sub(SHOWN_AT_MS.load(Ordering::SeqCst)) > BLUR_GRACE_MS
                {
                    INTENDED_VISIBLE.store(false, Ordering::SeqCst);
                    let _ = win_clone.hide();
                }
            }
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PinState;

    #[test]
    fn pinning_blocks_automatic_hides_only() {
        let pin = PinState::default();
        assert!(pin.allows_auto_hide(), "unpinned: auto-hide is allowed");

        pin.set(true);
        assert!(!pin.allows_auto_hide(), "pinned: nothing hides on its own");

        pin.set(false);
        assert!(pin.allows_auto_hide(), "unpinning restores auto-hide");
    }
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
