//! UI strings, loaded from `locales/*.json`.
//!
//! The files are the single source of truth for both sides of the app: the
//! frontend imports them directly, and `build.rs` embeds every one of them
//! here. Adding a language is dropping in `locales/<code>.json` — no code
//! in either runtime lists the available languages.
//!
//! Lookup is `section.key` with `{placeholder}` substitution, and falls
//! back to `FALLBACK_LANG` and then to the key itself, so a locale that is
//! missing a string degrades to English rather than to a blank label.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// Used when the selected language lacks a key, and when the OS language
/// matches nothing we ship.
const FALLBACK_LANG: &str = "en";

/// Index into `LOCALES` of the language in use. An index (rather than an
/// app handle threaded through every plugin) because the UI language is
/// process-wide state: plugins produce user-visible strings from deep
/// inside search, where no handle is available.
static CURRENT: AtomicUsize = AtomicUsize::new(usize::MAX);

type Catalog = HashMap<String, HashMap<String, String>>;

fn catalogs() -> &'static Vec<Catalog> {
    static PARSED: OnceLock<Vec<Catalog>> = OnceLock::new();
    PARSED.get_or_init(|| {
        LOCALES
            .iter()
            .map(|(code, json)| {
                serde_json::from_str::<Catalog>(json)
                    .unwrap_or_else(|e| panic!("locales/{}.json is malformed: {}", code, e))
            })
            .collect()
    })
}

// No `available()` / `display_name()` here on purpose: the language picker
// lives in the frontend, which embeds the same locale files (see
// src/lib/i18n.ts) and so already has the list and the `$meta.name` labels.
// A Rust copy would be a second source of truth for no reader.

fn code_static(code: &str) -> Option<&'static str> {
    LOCALES.iter().find(|(c, _)| *c == code).map(|(c, _)| *c)
}

fn index_of(code: &str) -> Option<usize> {
    LOCALES.iter().position(|(c, _)| *c == code)
}

/// Resolve a `language` setting: a shipped code wins, "system" (or
/// anything unknown) falls back to the OS language, then to English.
pub fn resolve(setting: &str) -> &'static str {
    if let Some(code) = code_static(setting) {
        return code;
    }
    os_language()
        .and_then(|os| {
            // "ja-JP" should find "ja"
            let base = os.split(['-', '_']).next().unwrap_or(&os).to_ascii_lowercase();
            code_static_owned(&base)
        })
        .unwrap_or(FALLBACK_LANG)
}

fn code_static_owned(base: &str) -> Option<&'static str> {
    LOCALES
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(base))
        .map(|(c, _)| *c)
}

#[cfg(windows)]
fn os_language() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if len <= 1 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..(len - 1) as usize]))
}

#[cfg(not(windows))]
fn os_language() -> Option<String> {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .ok()
        .filter(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

/// Set the language in use. Called at startup and whenever config changes.
pub fn set(setting: &str) {
    let code = resolve(setting);
    CURRENT.store(index_of(code).unwrap_or(0), Ordering::Relaxed);
}

/// The language code in use.
pub fn current() -> &'static str {
    let index = CURRENT.load(Ordering::Relaxed);
    LOCALES
        .get(index)
        .map(|(code, _)| *code)
        .unwrap_or(FALLBACK_LANG)
}

fn lookup(lang_index: usize, section: &str, key: &str) -> Option<&'static str> {
    catalogs()
        .get(lang_index)?
        .get(section)?
        .get(key)
        .map(|s| s.as_str())
}

/// `t("plugins", "copy_result")` — the string for the current language.
pub fn t(section: &str, key: &str) -> &'static str {
    let index = CURRENT.load(Ordering::Relaxed);
    if let Some(value) = lookup(index, section, key) {
        return value;
    }
    if let Some(fallback) = index_of(FALLBACK_LANG) {
        if let Some(value) = lookup(fallback, section, key) {
            return value;
        }
    }
    // Not `key` itself: that borrows the caller's &str, and callers want a
    // 'static. Leaking one string per missing key is bounded by the key
    // count and only happens on a bug in the locale files.
    Box::leak(key.to_string().into_boxed_str())
}

/// A whole section for the current language as a JSON object, with the
/// fallback language filling any gaps. Handed to tool windows, which have
/// no other way to reach the catalog.
pub fn section_json(section: &str) -> String {
    let mut merged: HashMap<&str, &str> = HashMap::new();
    for index in [index_of(FALLBACK_LANG), Some(CURRENT.load(Ordering::Relaxed))]
        .into_iter()
        .flatten()
    {
        if let Some(entries) = catalogs().get(index).and_then(|c| c.get(section)) {
            for (key, value) in entries {
                merged.insert(key.as_str(), value.as_str());
            }
        }
    }
    serde_json::to_string(&merged).unwrap_or_else(|_| "{}".into())
}

/// `t` plus `{placeholder}` substitution:
/// `tf("plugins", "calc_subtitle", &[("expr", "1+2")])`.
pub fn tf(section: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut out = t(section, key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{}}}", name), value);
    }
    out
}

/// "just now" / "5 min ago" / "2 h ago" / "3 d ago", in the current
/// language.
///
/// Here rather than in one of the plugins because two of them now say the
/// same thing about their own history — the clipboard's entries and the
/// captures — and a second copy of the thresholds is how the same age ends
/// up phrased two ways in one result list.
pub fn relative_time(seconds: u64) -> String {
    match seconds {
        0..=59 => t("plugins", "clipboard_now").into(),
        60..=3599 => tf("plugins", "clipboard_minutes", &[("n", &(seconds / 60).to_string())]),
        3600..=86399 => tf("plugins", "clipboard_hours", &[("n", &(seconds / 3600).to_string())]),
        _ => tf("plugins", "clipboard_days", &[("n", &(seconds / 86400).to_string())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `CURRENT` is process-wide, so tests that call `set` would otherwise
    /// see each other's language when cargo runs them in parallel.
    static LANG_LOCK: Mutex<()> = Mutex::new(());

    fn codes() -> Vec<&'static str> {
        LOCALES.iter().map(|(code, _)| *code).collect()
    }

    #[test]
    fn ships_at_least_japanese_and_english() {
        let codes = codes();
        assert!(codes.contains(&"ja"), "{:?}", codes);
        assert!(codes.contains(&"en"), "{:?}", codes);
    }

    /// A missing key in one language would silently show English (or the
    /// raw key) at runtime, which is hard to notice — catch it here.
    #[test]
    fn every_locale_defines_the_same_keys() {
        let reference = &catalogs()[index_of(FALLBACK_LANG).unwrap()];
        for (index, (code, _)) in LOCALES.iter().enumerate() {
            let catalog = &catalogs()[index];
            for (section, entries) in reference {
                let theirs = catalog
                    .get(section)
                    .unwrap_or_else(|| panic!("{}: missing section {}", code, section));
                for key in entries.keys() {
                    assert!(
                        theirs.contains_key(key),
                        "{}: missing {}.{}",
                        code,
                        section,
                        key
                    );
                }
            }
        }
    }

    #[test]
    fn resolves_settings_os_locales_and_junk() {
        assert_eq!(resolve("ja"), "ja");
        assert_eq!(resolve("en"), "en");
        // unknown / "system" fall through to the OS, then English
        assert!(codes().contains(&resolve("system")));
        assert!(codes().contains(&resolve("kl-GL")));
    }

    #[test]
    fn placeholders_are_substituted() {
        let _guard = LANG_LOCK.lock().unwrap();
        set("en");
        assert_eq!(tf("plugins", "calc_subtitle", &[("expr", "1+2")]), "= 1+2");
        // an unknown key degrades to the key, never to an empty label
        assert_eq!(t("plugins", "no_such_key"), "no_such_key");
    }

    #[test]
    fn switching_language_switches_strings() {
        let _guard = LANG_LOCK.lock().unwrap();
        set("ja");
        let ja = t("plugins", "copy_result");
        set("en");
        let en = t("plugins", "copy_result");
        assert_ne!(ja, en);
        assert_eq!(en, "Copy result");
    }

    #[test]
    fn section_json_carries_the_current_language() {
        let _guard = LANG_LOCK.lock().unwrap();
        set("ja");
        let json = section_json("tools");
        assert!(json.contains("Base64 変換"), "{}", json);
        set("en");
        assert!(section_json("tools").contains("Base64 converter"));
    }

    /// What `commands::config::resolved_language` hands the frontend, which
    /// uses it instead of resolving "system" from `navigator.languages`.
    #[test]
    fn current_reports_the_language_in_use() {
        let _guard = LANG_LOCK.lock().unwrap();
        set("ja");
        assert_eq!(current(), "ja");
        set("en");
        assert_eq!(current(), "en");
        set("system");
        assert!(codes().contains(&current()));
    }

    /// Both the clipboard history and the capture history date their
    /// entries with this, so the thresholds are worth pinning down.
    #[test]
    fn ages_are_phrased_by_how_old_they_are() {
        let _guard = LANG_LOCK.lock().unwrap();
        set("en");
        assert_eq!(relative_time(0), "just now");
        assert_eq!(relative_time(59), "just now");
        assert_eq!(relative_time(60), "1 min ago");
        assert_eq!(relative_time(3_600), "1 h ago");
        assert_eq!(relative_time(86_400 * 3), "3 d ago");
    }

    /// The frontend picker labels languages with `$meta.name`, so a locale
    /// without one would show up as a blank entry there.
    #[test]
    fn every_locale_names_itself() {
        for (index, (code, _)) in LOCALES.iter().enumerate() {
            let name = catalogs()[index]
                .get("$meta")
                .and_then(|meta| meta.get("name"))
                .unwrap_or_else(|| panic!("{}: no $meta.name", code));
            assert!(!name.trim().is_empty(), "{}: empty $meta.name", code);
        }
    }
}
