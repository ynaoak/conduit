//! Double-tap modifier key activation (e.g. Ctrl-Ctrl to open the launcher),
//! in the style of Listary / CLaunch.
//!
//! The global-shortcut plugin cannot express a bare modifier tap, let alone a
//! double tap, so on Windows we install a low-level keyboard hook
//! (WH_KEYBOARD_LL) on a dedicated thread with its own message loop.

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::OnceLock;
    use std::time::Instant;

    use tauri::AppHandle;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    use crate::window::toggle_launcher;

    /// Sentinel meaning "no previous tap recorded"
    const NO_TAP: u64 = u64::MAX;

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static EPOCH: OnceLock<Instant> = OnceLock::new();

    /// Left/right virtual-key codes of the target modifier
    static VK_LEFT: AtomicU32 = AtomicU32::new(0);
    static VK_RIGHT: AtomicU32 = AtomicU32::new(0);
    /// Max milliseconds between the two taps
    static INTERVAL_MS: AtomicU64 = AtomicU64::new(350);

    /// Timestamp (ms) of the previous completed tap
    static LAST_TAP_MS: AtomicU64 = AtomicU64::new(NO_TAP);
    /// The target key is currently held (used to ignore key-repeat)
    static KEY_HELD: AtomicBool = AtomicBool::new(false);
    /// A press is in flight and no other key has interrupted it
    static TAP_PENDING: AtomicBool = AtomicBool::new(false);

    fn now_ms() -> u64 {
        EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let msg = wparam.0 as u32;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            let is_target = kb.vkCode == VK_LEFT.load(Ordering::Relaxed)
                || kb.vkCode == VK_RIGHT.load(Ordering::Relaxed);

            if is_target {
                if is_down {
                    // swap returns the previous value: true means this is key-repeat
                    if !KEY_HELD.swap(true, Ordering::SeqCst) {
                        TAP_PENDING.store(true, Ordering::SeqCst);
                    }
                } else if is_up {
                    KEY_HELD.store(false, Ordering::SeqCst);
                    // Only a clean down→up with no other key in between counts
                    if TAP_PENDING.swap(false, Ordering::SeqCst) {
                        let now = now_ms();
                        let last = LAST_TAP_MS.load(Ordering::SeqCst);
                        if last != NO_TAP
                            && now.saturating_sub(last) <= INTERVAL_MS.load(Ordering::Relaxed)
                        {
                            LAST_TAP_MS.store(NO_TAP, Ordering::SeqCst);
                            if let Some(app) = APP.get() {
                                toggle_launcher(app);
                            }
                        } else {
                            LAST_TAP_MS.store(now, Ordering::SeqCst);
                        }
                    }
                }
            } else if is_down {
                // Any other key cancels the tap (Ctrl+C etc.) and the sequence
                TAP_PENDING.store(false, Ordering::SeqCst);
                LAST_TAP_MS.store(NO_TAP, Ordering::SeqCst);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// Install the double-tap hook. Returns false for unknown keys.
    pub fn install(app: &AppHandle, key: &str, interval_ms: u64) -> bool {
        let (left, right): (u32, u32) = match key.to_lowercase().as_str() {
            "ctrl" | "control" => (0xA2, 0xA3), // VK_LCONTROL / VK_RCONTROL
            "alt" => (0xA4, 0xA5),              // VK_LMENU / VK_RMENU
            "shift" => (0xA0, 0xA1),            // VK_LSHIFT / VK_RSHIFT
            "win" | "super" | "meta" => (0x5B, 0x5C), // VK_LWIN / VK_RWIN
            _ => return false,
        };

        let _ = APP.set(app.clone());
        VK_LEFT.store(left, Ordering::Relaxed);
        VK_RIGHT.store(right, Ordering::Relaxed);
        INTERVAL_MS.store(interval_ms.clamp(100, 2000), Ordering::Relaxed);

        // WH_KEYBOARD_LL needs a thread with a message loop
        std::thread::spawn(|| unsafe {
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) {
                Ok(_) => {
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                }
                Err(e) => eprintln!("Failed to install double-tap keyboard hook: {}", e),
            }
        });
        true
    }
}

#[cfg(not(windows))]
mod imp {
    use tauri::AppHandle;

    pub fn install(_app: &AppHandle, _key: &str, _interval_ms: u64) -> bool {
        false
    }
}

pub use imp::install;
