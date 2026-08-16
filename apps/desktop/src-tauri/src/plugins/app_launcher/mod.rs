#[cfg(windows)]
mod icons;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

/// Re-scan installed apps periodically so newly installed apps show up
const RESCAN_INTERVAL: Duration = Duration::from_secs(300);
const MAX_RESULTS: usize = 10;
/// Fuzzy score contributes up to 0.9; launch-count bonus up to 0.1
const COUNT_BONUS_PER_LAUNCH: f64 = 0.005;
const COUNT_BONUS_CAP: f64 = 0.1;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone)]
enum LaunchTarget {
    /// Start Menu shortcut or executable
    Path(PathBuf),
    /// UWP / Store app AUMID, launched via shell:AppsFolder
    ShellApp(String),
}

#[derive(Clone)]
struct AppEntry {
    name: String,
    /// Kana-normalized name (katakana -> hiragana), present only when the
    /// name contains kana. Matched against romaji-converted queries.
    name_kana: Option<String>,
    target: LaunchTarget,
}

impl AppEntry {
    fn new(name: String, target: LaunchTarget) -> Self {
        let name_kana = crate::ja::contains_kana(&name).then(|| crate::ja::normalize_kana(&name));
        Self {
            name,
            name_kana,
            target,
        }
    }

    fn id(&self) -> String {
        match &self.target {
            LaunchTarget::Path(p) => format!("conduit.app-launcher:path:{}", p.display()),
            LaunchTarget::ShellApp(aumid) => format!("conduit.app-launcher:uwp:{}", aumid),
        }
    }

    fn subtitle(&self) -> String {
        match &self.target {
            LaunchTarget::Path(p) => p.display().to_string(),
            LaunchTarget::ShellApp(_) => crate::i18n::t("plugins", "store_app").into(),
        }
    }

    fn emoji(&self) -> &'static str {
        match &self.target {
            LaunchTarget::Path(_) => "🚀",
            LaunchTarget::ShellApp(_) => "🪟",
        }
    }

    fn actions(&self) -> Vec<Action> {
        let mut actions = vec![Action {
            id: "open".into(),
            title: crate::i18n::t("plugins", "open").into(),
            shortcut: Some("Enter".into()),
        }];
        if matches!(self.target, LaunchTarget::Path(_)) {
            actions.push(Action {
                id: "open-folder".into(),
                title: crate::i18n::t("plugins", "open_folder").into(),
                shortcut: None,
            });
            actions.push(Action {
                id: "copy-path".into(),
                title: crate::i18n::t("plugins", "copy_path").into(),
                shortcut: None,
            });
        }
        actions
    }
}

/// Shape of `Get-StartApps | ConvertTo-Json` entries
#[derive(Deserialize)]
struct StartApp {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

/// Path -> base64 PNG (None = extraction failed, use the fallback emoji)
type IconCache = Arc<std::sync::RwLock<HashMap<String, Option<String>>>>;

pub struct AppLauncherPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    apps: Arc<RwLock<Vec<AppEntry>>>,
    /// App name -> launch count, persisted to launch_counts.json
    launch_counts: Arc<RwLock<HashMap<String, u32>>>,
    icon_cache: IconCache,
    matcher: SkimMatcherV2,
}

impl AppLauncherPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.app-launcher".into(),
                name: "Applications".into(),
                description: "Search and launch applications".into(),
                icon: "apps".into(),
                keyword: None,
                keyword_only: false,
            },
            app,
            apps: Arc::new(RwLock::new(Vec::new())),
            launch_counts: Arc::new(RwLock::new(HashMap::new())),
            icon_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Real app icon when extractable, fallback emoji otherwise
    fn resolve_icon(&self, app: &AppEntry) -> ResultIcon {
        #[cfg(windows)]
        if let LaunchTarget::Path(path) = &app.target {
            let key = path.display().to_string();
            if let Some(cached) = self.icon_cache.read().unwrap().get(&key).cloned() {
                return match cached {
                    Some(b64) => ResultIcon::Base64(b64),
                    None => ResultIcon::Emoji(app.emoji().into()),
                };
            }
            let extracted = icons::extract_icon_base64(&key);
            self.icon_cache
                .write()
                .unwrap()
                .insert(key, extracted.clone());
            if let Some(b64) = extracted {
                return ResultIcon::Base64(b64);
            }
        }
        ResultIcon::Emoji(app.emoji().into())
    }

    fn counts_path(&self) -> Option<PathBuf> {
        self.app
            .path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("launch_counts.json"))
    }
}

fn scan_start_menu() -> Vec<AppEntry> {
    let mut dirs = Vec::new();

    // Common Start Menu
    if let Ok(programdata) = std::env::var("ProgramData") {
        dirs.push(PathBuf::from(programdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }

    // User Start Menu
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }

    let mut apps = Vec::new();
    for dir in dirs {
        if dir.exists() {
            scan_directory(&dir, &mut apps);
        }
    }
    apps
}

fn scan_directory(dir: &Path, apps: &mut Vec<AppEntry>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory(&path, apps);
            } else if let Some(ext) = path.extension() {
                if ext == "lnk" || ext == "exe" {
                    if let Some(name) = path.file_stem() {
                        let name_str = name.to_string_lossy().to_string();
                        // Skip uninstallers
                        if name_str.to_lowercase().contains("uninstall") {
                            continue;
                        }
                        apps.push(AppEntry::new(name_str, LaunchTarget::Path(path.clone())));
                    }
                }
            }
        }
    }
}

/// Enumerate Start apps (incl. UWP/Store apps) via PowerShell Get-StartApps.
/// Returns an empty list if PowerShell is unavailable or the output is unparsable.
fn scan_shell_apps() -> Vec<AppEntry> {
    let mut cmd = std::process::Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-StartApps | ConvertTo-Json -Compress",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let Ok(output) = cmd.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let json = String::from_utf8_lossy(&output.stdout);
    // ConvertTo-Json emits a bare object (not an array) when there is a single entry
    let entries: Vec<StartApp> = serde_json::from_str::<Vec<StartApp>>(&json)
        .or_else(|_| serde_json::from_str::<StartApp>(&json).map(|a| vec![a]))
        .unwrap_or_default();

    entries
        .into_iter()
        .map(|a| AppEntry::new(a.name, LaunchTarget::ShellApp(a.app_id)))
        .collect()
}

/// Full scan: Start Menu entries first, then shell apps deduped by name
fn scan_all_apps() -> Vec<AppEntry> {
    let mut apps = scan_start_menu();
    let seen: HashSet<String> = apps.iter().map(|a| a.name.to_lowercase()).collect();

    for app in scan_shell_apps() {
        if !seen.contains(&app.name.to_lowercase()) {
            apps.push(app);
        }
    }
    apps
}

fn load_counts(path: &Option<PathBuf>) -> HashMap<String, u32> {
    let Some(path) = path else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_counts(path: &Option<PathBuf>, counts: &HashMap<String, u32>) {
    let Some(path) = path else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(counts) {
        let _ = std::fs::write(path, json);
    }
}

#[async_trait]
impl ConduitPlugin for AppLauncherPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        // Load persisted launch counts
        *self.launch_counts.write().await = load_counts(&self.counts_path());

        // Quick synchronous scan so results are available immediately
        *self.apps.write().await = scan_start_menu();

        // Slow scan (PowerShell for UWP apps) + periodic rescan in the background
        let apps = self.apps.clone();
        let icon_cache = self.icon_cache.clone();
        tauri::async_runtime::spawn(async move {
            #[cfg(not(windows))]
            let _ = &icon_cache;
            loop {
                let scanned = tokio::task::spawn_blocking(scan_all_apps)
                    .await
                    .unwrap_or_default();
                if !scanned.is_empty() {
                    *apps.write().await = scanned;
                }

                // Pre-extract icons so search/browse hit the cache
                #[cfg(windows)]
                {
                    let paths: Vec<String> = apps
                        .read()
                        .await
                        .iter()
                        .filter_map(|a| match &a.target {
                            LaunchTarget::Path(p) => Some(p.display().to_string()),
                            LaunchTarget::ShellApp(_) => None,
                        })
                        .collect();
                    let cache = icon_cache.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        for path in paths {
                            if cache.read().unwrap().contains_key(&path) {
                                continue;
                            }
                            let icon = icons::extract_icon_base64(&path);
                            cache.write().unwrap().insert(path, icon);
                        }
                    })
                    .await;
                }

                tokio::time::sleep(RESCAN_INTERVAL).await;
            }
        });

        Ok(())
    }

    async fn browse(&self) -> Vec<SearchResult> {
        let apps = self.apps.read().await;
        let counts = self.launch_counts.read().await;

        let mut entries: Vec<&AppEntry> = apps.iter().collect();
        // Most-launched first, then alphabetical
        entries.sort_by(|a, b| {
            let count_a = counts.get(&a.name).copied().unwrap_or(0);
            let count_b = counts.get(&b.name).copied().unwrap_or(0);
            count_b
                .cmp(&count_a)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        entries
            .iter()
            .map(|app| {
                let count = counts.get(&app.name).copied().unwrap_or(0);
                let subtitle = if count > 0 {
                    format!("{} ・ {} 回起動", app.subtitle(), count)
                } else {
                    app.subtitle()
                };
                SearchResult {
                    id: app.id(),
                    plugin_id: "conduit.app-launcher".into(),
                    title: app.name.clone(),
                    subtitle: Some(subtitle),
                    icon: self.resolve_icon(app),
                    score: 0.0,
                    actions: app.actions(),
                    match_indices: vec![],
                }
            })
            .collect()
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let apps = self.apps.read().await;
        let counts = self.launch_counts.read().await;

        // Empty query: CLaunch-style favorites — most launched apps first
        if query.trim().is_empty() {
            let mut favorites: Vec<(&AppEntry, u32)> = apps
                .iter()
                .filter_map(|app| {
                    counts
                        .get(&app.name)
                        .copied()
                        .filter(|&c| c > 0)
                        .map(|c| (app, c))
                })
                .collect();
            favorites.sort_by(|a, b| b.1.cmp(&a.1));
            favorites.truncate(8);

            return favorites
                .iter()
                .enumerate()
                .map(|(i, (app, count))| SearchResult {
                    id: app.id(),
                    plugin_id: "conduit.app-launcher".into(),
                    title: app.name.clone(),
                    subtitle: Some(format!("{} 回起動", count)),
                    icon: self.resolve_icon(app),
                    score: 0.9 - i as f64 * 0.05,
                    actions: app.actions(),
                    match_indices: vec![],
                })
                .collect();
        }

        // Romaji queries also match kana names: "memo" -> めも hits メモ帳
        let kana_variants = crate::ja::query_variants(&query.to_lowercase());

        let mut results: Vec<SearchResult> = apps
            .iter()
            .filter_map(|app| {
                // Direct match drives highlighting
                let direct = self.matcher.fuzzy_indices(&app.name, query);
                let mut best = direct.as_ref().map(|(s, _)| *s);

                // Kana-space match (no highlight — indices don't map back)
                if let Some(kana_name) = &app.name_kana {
                    for variant in &kana_variants {
                        best = best.max(self.matcher.fuzzy_match(kana_name, variant));
                    }
                }

                best.map(|score| {
                    let base = (score as f64 / 100.0).min(0.9).max(0.0);
                    // Frequently launched apps rank higher
                    let count = counts.get(&app.name).copied().unwrap_or(0);
                    let bonus = (count as f64 * COUNT_BONUS_PER_LAUNCH).min(COUNT_BONUS_CAP);
                    SearchResult {
                        id: app.id(),
                        plugin_id: "conduit.app-launcher".into(),
                        title: app.name.clone(),
                        subtitle: Some(app.subtitle()),
                        icon: self.resolve_icon(app),
                        score: (base + bonus).min(1.0),
                        actions: app.actions(),
                        match_indices: direct.map(|(_, indices)| indices).unwrap_or_default(),
                    }
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(MAX_RESULTS);
        results
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let target = result_id
            .strip_prefix("conduit.app-launcher:")
            .unwrap_or(result_id);
        let path = target
            .strip_prefix("path:")
            .or_else(|| target.strip_prefix("uwp:"))
            .unwrap_or(target);

        match action_id {
            "copy-path" => {
                use tauri_plugin_clipboard_manager::ClipboardExt;
                self.app
                    .clipboard()
                    .write_text(path.to_string())
                    .map_err(|e| anyhow::anyhow!("failed to copy path: {}", e))?;
                return Ok(());
            }
            "open-folder" => {
                let mut cmd = std::process::Command::new("explorer.exe");
                cmd.arg(format!("/select,{}", path));
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(CREATE_NO_WINDOW);
                }
                cmd.spawn()?;
                return Ok(());
            }
            _ => {}
        }

        // Default: launch the app
        let mut cmd = if let Some(aumid) = target.strip_prefix("uwp:") {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(format!("shell:AppsFolder\\{}", aumid));
            c
        } else {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "start", "", path]);
            c
        };
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.spawn()?;

        // Record the launch for frequency-based ranking
        let apps = self.apps.read().await;
        if let Some(entry) = apps.iter().find(|a| a.id() == result_id) {
            let name = entry.name.clone();
            drop(apps);
            let mut counts = self.launch_counts.write().await;
            *counts.entry(name).or_insert(0) += 1;
            save_counts(&self.counts_path(), &counts);
        }

        Ok(())
    }
}
