use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub hotkey: HotkeyConfig,
    pub keybindings: KeybindingsConfig,
    pub theme: ThemeConfig,
    pub plugins: PluginsConfig,
    pub search: SearchConfig,
    pub window: WindowConfig,
    pub workflows: WorkflowsConfig,
    /// UI language: a code shipped in locales/ (e.g. "ja", "en") or
    /// "system" to follow the OS
    pub language: String,
    /// Look for a signed update shortly after launch. Only ever notifies —
    /// installing is a separate confirmation every time.
    pub auto_update_check: bool,
}

/// In-app keyboard shortcuts. Each action accepts multiple chords like
/// "ArrowDown", "Ctrl+N", "Shift+Tab". Consumed by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub move_up: Vec<String>,
    pub move_down: Vec<String>,
    pub execute: Vec<String>,
    pub action_panel: Vec<String>,
    pub close: Vec<String>,
    /// Toggle the selected result as a pinned favorite
    pub toggle_pin: Vec<String>,
    /// Toggle the manage view (workflows / launcher / key settings)
    pub manage_view: Vec<String>,
    /// Cycle tabs inside the manage view
    pub tab_next: Vec<String>,
    pub tab_prev: Vec<String>,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            move_up: vec!["ArrowUp".into(), "Ctrl+P".into()],
            move_down: vec!["ArrowDown".into(), "Ctrl+N".into()],
            execute: vec!["Enter".into()],
            // Bare "Shift" is a tap: press and release without another key
            action_panel: vec!["Shift".into(), "Ctrl+K".into()],
            close: vec!["Escape".into()],
            toggle_pin: vec!["Ctrl+D".into()],
            manage_view: vec!["Ctrl+,".into()],
            tab_next: vec!["Tab".into(), "ArrowRight".into(), "Ctrl+Tab".into()],
            tab_prev: vec![
                "Shift+Tab".into(),
                "ArrowLeft".into(),
                "Ctrl+Shift+Tab".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// Modifier key: "Alt", "Ctrl", "Shift", "Super", or combinations like "Ctrl+Shift"
    pub modifier: String,
    /// Key: "Space", "N", "K", etc.
    pub key: String,
    /// Double-tap activation key: "Ctrl", "Alt", "Shift", "Win", or "" to disable.
    /// Tapping the key twice in quick succession toggles the launcher.
    pub double_tap: String,
    /// Max interval between the two taps (milliseconds)
    pub double_tap_interval_ms: u64,
}

/// Theme = a mode plus optional per-slot color overrides. The built-in
/// Material 3 dark and light palettes live in src/styles/global.css; a slot
/// set here overrides both. Configs written by older versions carry the
/// full palette as concrete values, so existing customized (or default)
/// looks survive as overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// "system" (follow the OS), "dark", or "light"
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_hover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg_selected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_primary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    /// Map of plugin_id -> enabled
    pub enabled: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Maximum number of results to show
    pub max_results: usize,
    /// Web search engine: "google", "duckduckgo", "bing", or a custom URL with {query} placeholder
    pub web_search_engine: String,
}

/// Per-package workflow state (the packages themselves live on disk)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowsConfig {
    /// Package ids excluded from search and launching. Built-in packages
    /// cannot be deleted, so disabling is their only off switch.
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Hide window when it loses focus
    pub hide_on_blur: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig::default(),
            keybindings: KeybindingsConfig::default(),
            theme: ThemeConfig::default(),
            plugins: PluginsConfig::default(),
            search: SearchConfig::default(),
            window: WindowConfig::default(),
            workflows: WorkflowsConfig::default(),
            language: "system".into(),
            auto_update_check: true,
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            // Option+Space on macOS, Alt+Space elsewhere — the same physical
            // chord, since Modifiers::ALT is Option there. It is the one free
            // single-modifier combination on a Mac: Cmd+Space is Spotlight and
            // Ctrl+Space switches input sources, which would be a daily
            // collision for anyone typing Japanese.
            modifier: "Alt".into(),
            key: "Space".into(),
            // The double-tap detector is Windows-only (see double_tap.rs), so
            // asking for one anywhere else just produces a startup warning
            // about a feature that cannot run.
            double_tap: if cfg!(windows) { "Ctrl".into() } else { String::new() },
            double_tap_interval_ms: 350,
        }
    }
}

/// How to describe the summon chord to a user, in that platform's own names.
/// "Alt+Space" is the correct config value on a Mac but not what is printed
/// on the key, and a launcher whose instructions name a key you cannot find
/// is no better than no instructions.
pub fn summon_label(hotkey: &HotkeyConfig) -> String {
    let modifier = hotkey
        .modifier
        .split('+')
        .map(|part| match part.trim().to_lowercase().as_str() {
            "alt" if cfg!(target_os = "macos") => "Option",
            "alt" => "Alt",
            "ctrl" | "control" => "Control",
            "shift" => "Shift",
            "super" | "win" | "meta" | "cmd" if cfg!(target_os = "macos") => "Command",
            "super" | "win" | "meta" | "cmd" => "Win",
            other => Box::leak(other.to_string().into_boxed_str()),
        })
        .collect::<Vec<_>>()
        .join("+");
    format!("{}+{}", modifier, hotkey.key)
}

#[cfg(test)]
mod hotkey_tests {
    use super::*;

    /// Only Windows has a double-tap detector; defaulting other platforms to
    /// a key they cannot listen for produces a warning about a feature that
    /// was never going to run.
    #[test]
    fn double_tap_defaults_off_where_it_cannot_work() {
        let defaults = HotkeyConfig::default();
        if cfg!(windows) {
            assert_eq!(defaults.double_tap, "Ctrl");
        } else {
            assert!(defaults.double_tap.is_empty(), "{:?}", defaults.double_tap);
        }
    }

    #[test]
    fn the_summon_chord_is_named_in_platform_terms() {
        let label = summon_label(&HotkeyConfig::default());
        if cfg!(target_os = "macos") {
            assert_eq!(label, "Option+Space");
        } else {
            assert_eq!(label, "Alt+Space");
        }
    }

    #[test]
    fn compound_modifiers_are_translated_part_by_part() {
        let hotkey = HotkeyConfig {
            modifier: "Cmd+Shift".into(),
            key: "Space".into(),
            ..HotkeyConfig::default()
        };
        let label = summon_label(&hotkey);
        assert!(label.ends_with("+Shift+Space"), "{}", label);
        assert!(label.starts_with(if cfg!(target_os = "macos") { "Command" } else { "Win" }));
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: "system".into(),
            bg_primary: None,
            bg_secondary: None,
            bg_hover: None,
            bg_selected: None,
            text_primary: None,
            text_secondary: None,
            text_accent: None,
            border_color: None,
            border_radius: None,
        }
    }
}

impl Default for PluginsConfig {
    fn default() -> Self {
        let mut enabled = HashMap::new();
        enabled.insert("conduit.app-launcher".into(), true);
        enabled.insert("conduit.base64".into(), true);
        enabled.insert("conduit.calculator".into(), true);
        enabled.insert("conduit.clipboard-history".into(), true);
        enabled.insert("conduit.json".into(), true);
        enabled.insert("conduit.system-commands".into(), true);
        enabled.insert("conduit.web-search".into(), true);
        enabled.insert("conduit.workflows".into(), true);
        enabled.insert("conduit.file-search".into(), true);
        enabled.insert("conduit.window-switcher".into(), true);
        enabled.insert("conduit.process-monitor".into(), true);
        Self { enabled }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_results: 20,
            web_search_engine: "google".into(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            hide_on_blur: true,
        }
    }
}

/// Shared config state
#[derive(Clone)]
pub struct ConfigState {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
}

impl ConfigState {
    pub fn load(config_dir: &PathBuf) -> Self {
        let config_path = config_dir.join("config.json");
        let config = if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => match serde_json::from_str::<AppConfig>(&contents) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("Failed to parse config.json: {}, using defaults", e);
                        AppConfig::default()
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read config.json: {}, using defaults", e);
                    AppConfig::default()
                }
            }
        } else {
            // Write default config for user reference
            let config = AppConfig::default();
            if let Ok(json) = serde_json::to_string_pretty(&config) {
                let _ = std::fs::create_dir_all(config_dir);
                let _ = std::fs::write(&config_path, json);
            }
            config
        };

        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        }
    }

    pub async fn get(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub async fn update(&self, new_config: AppConfig) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&new_config)?;
        std::fs::write(&self.config_path, &json)?;
        *self.config.write().await = new_config;
        Ok(())
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }
}

/// Resolve a web search engine name to a URL template
pub fn resolve_search_url(engine: &str, query: &str) -> String {
    let encoded = urlencoding::encode(query);
    match engine.to_lowercase().as_str() {
        "google" => format!("https://www.google.com/search?q={}", encoded),
        "duckduckgo" | "ddg" => format!("https://duckduckgo.com/?q={}", encoded),
        "bing" => format!("https://www.bing.com/search?q={}", encoded),
        custom if custom.contains("{query}") => custom.replace("{query}", &encoded),
        _ => format!("https://www.google.com/search?q={}", encoded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `serde_json` runs with `arbitrary_precision` for the JSON formatter,
    /// which changes how numbers are represented. config.json holds numbers
    /// (`max_results`, `double_tap_interval_ms`), so guard the round trip.
    #[test]
    fn config_survives_a_json_round_trip() {
        let json = serde_json::to_string_pretty(&AppConfig::default()).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.search.max_results, 20);
        assert_eq!(parsed.hotkey.double_tap_interval_ms, 350);
        // The default is platform-dependent; what this guards is that the
        // round trip returns it unchanged, not which value it happens to be.
        assert_eq!(parsed.hotkey.double_tap, HotkeyConfig::default().double_tap);
        assert_eq!(parsed.hotkey.modifier, "Alt");
        assert_eq!(parsed.plugins.enabled.get("conduit.json"), Some(&true));
    }

    #[test]
    fn theme_mode_defaults_to_system_and_old_palettes_become_overrides() {
        let parsed: AppConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.theme.mode, "system");
        assert_eq!(parsed.theme.bg_primary, None);

        // a config written by an older version: full palette, no mode
        let parsed: AppConfig =
            serde_json::from_str(r##"{"theme":{"bg_primary":"#1a1a2e"}}"##).unwrap();
        assert_eq!(parsed.theme.mode, "system");
        assert_eq!(parsed.theme.bg_primary.as_deref(), Some("#1a1a2e"));

        // unset slots stay omitted on write so palette switching keeps working
        let json = serde_json::to_string(&AppConfig::default()).unwrap();
        assert!(!json.contains("bg_primary"));
        assert!(json.contains(r#""mode":"system""#));
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: AppConfig = serde_json::from_str(r#"{"search":{"max_results":5}}"#).unwrap();

        assert_eq!(parsed.search.max_results, 5);
        assert_eq!(parsed.search.web_search_engine, "google");
        assert_eq!(parsed.hotkey.key, "Space");
    }
}

/// The updater manifest that `scripts/release-public.ps1` publishes as
/// `latest.json`.
///
/// The script builds that JSON by hand — on Windows there is no CI step
/// (tauri-action) to generate it — so nothing else checks that its shape is
/// the one `tauri-plugin-updater` will accept. A wrong field name or
/// platform key fails silently in the worst way: the manifest parses or the
/// lookup misses, and users simply never see an update. These tests parse a
/// copy of the script's output with the plugin's own type.
#[cfg(test)]
mod updater_manifest_tests {
    use tauri_plugin_updater::RemoteRelease;

    /// Must mirror the object built in scripts/release-public.ps1.
    const SAMPLE: &str = r#"{
      "version": "0.2.0",
      "notes": "conduit v0.2.0",
      "pub_date": "2026-08-11T06:12:00Z",
      "platforms": {
        "windows-x86_64": {
          "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQpSVUE=",
          "url": "https://github.com/ynaoak/conduit/releases/download/v0.2.0/conduit_0.2.0_x64-setup.exe"
        }
      }
    }"#;

    #[test]
    fn the_published_manifest_shape_is_the_one_the_plugin_reads() {
        let release: RemoteRelease = serde_json::from_str(SAMPLE).expect("manifest must parse");
        assert_eq!(release.version.to_string(), "0.2.0");
        // The key the plugin composes for a 64-bit Windows install is
        // "{os}-{arch}" = windows-x86_64. Getting this wrong parses fine and
        // then never matches, which is why it is asserted rather than assumed.
        let url = release
            .download_url("windows-x86_64")
            .expect("windows-x86_64 must be present");
        assert!(url.as_str().ends_with("-setup.exe"), "{}", url);
        assert!(release.signature("windows-x86_64").is_ok());
    }

    /// macOS is distributed as a notarised dmg rather than through the Mac
    /// App Store (see docs/store-distribution.md), so when a macOS build
    /// exists its entries go in this same manifest. The keys are what the
    /// plugin composes from `updater_os()`/`updater_arch()` — pinned here
    /// because a wrong one is invisible: the JSON stays valid and the
    /// lookup simply never matches.
    #[test]
    fn a_macos_build_would_key_off_darwin() {
        let with_mac = SAMPLE.replace(
            r#""platforms": {"#,
            r#""platforms": {
              "darwin-aarch64": {
                "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQpSVUE=",
                "url": "https://github.com/ynaoak/conduit/releases/download/v0.2.0/Conduit_0.2.0_aarch64.app.tar.gz"
              },"#,
        );
        let release: RemoteRelease = serde_json::from_str(&with_mac).expect("manifest must parse");
        assert!(release.download_url("darwin-aarch64").is_ok());
        // the Windows entry must survive alongside it
        assert!(release.download_url("windows-x86_64").is_ok());
        // "macos-*" is the name people reach for and is not what the plugin asks for
        assert!(release.download_url("macos-aarch64").is_err());
    }

    /// pub_date is parsed as RFC3339 and a bad value rejects the whole
    /// manifest, so the format the script writes has to be exactly this.
    #[test]
    fn pub_date_must_be_rfc3339() {
        assert!(serde_json::from_str::<RemoteRelease>(SAMPLE).is_ok());
        let bad = SAMPLE.replace("2026-08-11T06:12:00Z", "2026/08/11 06:12:00");
        assert!(serde_json::from_str::<RemoteRelease>(&bad).is_err());
    }
}
