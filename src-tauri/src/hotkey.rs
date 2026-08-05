use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::config::HotkeyConfig;
use crate::window::toggle_launcher;

/// Returns true if hotkey was registered successfully.
pub fn register_global_shortcut(app: &AppHandle, hotkey_config: &HotkeyConfig) -> bool {
    let modifiers = parse_modifiers(&hotkey_config.modifier);
    let code = parse_code(&hotkey_config.key);
    let shortcut = Shortcut::new(modifiers, code);

    // Unregister first in case it's already registered (e.g. hot-reload)
    let _ = app.global_shortcut().unregister(shortcut);

    match app
        .global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_launcher(app);
            }
        }) {
        Ok(_) => true,
        Err(e) => {
            eprintln!(
                "Warning: Failed to register hotkey {}+{}: {}",
                hotkey_config.modifier, hotkey_config.key, e
            );
            false
        }
    }
}

fn parse_modifiers(s: &str) -> Option<Modifiers> {
    let mut mods = Modifiers::empty();
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    for part in parts {
        match part.to_lowercase().as_str() {
            "alt" => mods |= Modifiers::ALT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "win" | "meta" | "cmd" => mods |= Modifiers::SUPER,
            _ => {}
        }
    }
    if mods.is_empty() {
        None
    } else {
        Some(mods)
    }
}

fn parse_code(s: &str) -> Code {
    match s.to_lowercase().as_str() {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        _ => Code::Space,
    }
}
