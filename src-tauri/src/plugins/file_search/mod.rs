use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::RwLock;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const MAX_RESULTS: usize = 15;
/// Fallback index limits
const MAX_INDEX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 3;
const REINDEX_INTERVAL: Duration = Duration::from_secs(600);

#[derive(Clone)]
struct FileEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

pub struct FileSearchPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    /// Fallback index of common user folders, used when Everything is unavailable
    index: Arc<RwLock<Vec<FileEntry>>>,
    matcher: SkimMatcherV2,
}

impl FileSearchPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.file-search".into(),
                name: "File Search".into(),
                description: "Search files via Everything or the local index".into(),
                icon: "file".into(),
                keyword: Some("f".into()),
                // es.exe spawns per query; keep it off the general fan-out
                keyword_only: true,
            },
            app,
            index: Arc::new(RwLock::new(Vec::new())),
            matcher: SkimMatcherV2::default(),
        }
    }

    fn path_to_result(
        path: &Path,
        is_dir: bool,
        score: f64,
        match_indices: Vec<usize>,
    ) -> SearchResult {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        SearchResult {
            id: format!("conduit.file-search:{}", path.display()),
            plugin_id: "conduit.file-search".into(),
            title: name,
            subtitle: Some(path.display().to_string()),
            icon: ResultIcon::Emoji(if is_dir { "📁" } else { "📄" }.into()),
            score,
            actions: vec![
                Action {
                    id: "open".into(),
                    title: "開く".into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "open-folder".into(),
                    title: "フォルダで開く".into(),
                    shortcut: None,
                },
                Action {
                    id: "copy-path".into(),
                    title: "パスをコピー".into(),
                    shortcut: None,
                },
            ],
            match_indices,
        }
    }
}

/// Query Everything via es.exe. Results are exported to a temp file because
/// es.exe writes stdout in the console codepage (garbles Japanese paths),
/// while -export-txt writes UTF-8.
fn es_search(query: &str) -> Option<Vec<PathBuf>> {
    let tmp = std::env::temp_dir().join("conduit_es_results.txt");

    let mut cmd = std::process::Command::new("es.exe");
    cmd.args(["-n", "15", "-export-txt"])
        .arg(&tmp)
        .args(query.split_whitespace());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let bytes = std::fs::read(&tmp).ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let text = String::from_utf8_lossy(bytes);

    Some(
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

/// Build the fallback index from common user folders
fn build_index() -> Vec<FileEntry> {
    let Ok(profile) = std::env::var("USERPROFILE") else {
        return Vec::new();
    };
    let profile = PathBuf::from(profile);

    let mut entries = Vec::new();
    for dir in ["Desktop", "Documents", "Downloads", "Pictures", "Videos", "Music"] {
        let root = profile.join(dir);
        if root.exists() {
            walk(&root, 0, &mut entries);
        }
        if entries.len() >= MAX_INDEX_ENTRIES {
            break;
        }
    }
    entries
}

fn walk(dir: &Path, depth: usize, entries: &mut Vec<FileEntry>) {
    if depth > MAX_DEPTH || entries.len() >= MAX_INDEX_ENTRIES {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        if entries.len() >= MAX_INDEX_ENTRIES {
            return;
        }
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        // Skip hidden and dependency directories
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let is_dir = path.is_dir();
        entries.push(FileEntry {
            name,
            path: path.clone(),
            is_dir,
        });
        if is_dir {
            walk(&path, depth + 1, entries);
        }
    }
}

#[async_trait]
impl ConduitPlugin for FileSearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        // Build and periodically refresh the fallback index in the background
        let index = self.index.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let built = tokio::task::spawn_blocking(build_index)
                    .await
                    .unwrap_or_default();
                *index.write().await = built;
                tokio::time::sleep(REINDEX_INTERVAL).await;
            }
        });
        Ok(())
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return vec![];
        }

        // Prefer Everything when available: full-drive instant search
        let owned = query.to_string();
        if let Ok(Some(paths)) = tokio::task::spawn_blocking(move || es_search(&owned)).await {
            return paths
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    Self::path_to_result(p, p.is_dir(), 0.95 - i as f64 * 0.03, vec![])
                })
                .collect();
        }

        // Fallback: fuzzy match against the local index
        let index = self.index.read().await;
        let mut scored: Vec<(i64, Vec<usize>, &FileEntry)> = index
            .iter()
            .filter_map(|e| {
                self.matcher
                    .fuzzy_indices(&e.name, query)
                    .map(|(s, indices)| (s, indices, e))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(MAX_RESULTS);

        scored
            .into_iter()
            .map(|(s, indices, e)| {
                let normalized = (s as f64 / 100.0).min(0.9).max(0.0);
                Self::path_to_result(&e.path, e.is_dir, normalized, indices)
            })
            .collect()
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let path = result_id
            .strip_prefix("conduit.file-search:")
            .unwrap_or(result_id);

        match action_id {
            "copy-path" => {
                use tauri_plugin_clipboard_manager::ClipboardExt;
                self.app
                    .clipboard()
                    .write_text(path.to_string())
                    .map_err(|e| anyhow::anyhow!("failed to copy path: {}", e))?;
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
            }
            _ => {
                self.app.opener().open_path(path, None::<&str>)?;
            }
        }
        Ok(())
    }
}
