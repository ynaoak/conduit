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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub bg_primary: String,
    pub bg_secondary: String,
    pub bg_hover: String,
    pub bg_selected: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub text_accent: String,
    pub border_color: String,
    pub border_radius: String,
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
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifier: "Alt".into(),
            key: "Space".into(),
            double_tap: "Ctrl".into(),
            double_tap_interval_ms: 350,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            bg_primary: "#1a1a2e".into(),
            bg_secondary: "#16213e".into(),
            bg_hover: "#1e2d4a".into(),
            bg_selected: "#2a3f5f".into(),
            text_primary: "#e8e8ed".into(),
            text_secondary: "#8e8ea0".into(),
            text_accent: "#7c83fd".into(),
            border_color: "#2a2a4a".into(),
            border_radius: "12px".into(),
        }
    }
}

impl Default for PluginsConfig {
    fn default() -> Self {
        let mut enabled = HashMap::new();
        enabled.insert("conduit.app-launcher".into(), true);
        enabled.insert("conduit.calculator".into(), true);
        enabled.insert("conduit.clipboard-history".into(), true);
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
