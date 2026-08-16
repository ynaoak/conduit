use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::RwLock;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

/// Maximum number of entries kept in history
const MAX_ENTRIES: usize = 50;
/// Clipboard polling interval
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// Number of entries shown for an empty query (bare "cb" keyword)
const DEFAULT_LISTING: usize = 10;
/// Title preview length (chars)
const PREVIEW_LEN: usize = 60;

#[derive(Clone)]
struct ClipEntry {
    text: String,
    copied_at: SystemTime,
}

impl ClipEntry {
    fn hash_id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.text.hash(&mut hasher);
        hasher.finish()
    }
}

pub struct ClipboardHistoryPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    history: Arc<RwLock<VecDeque<ClipEntry>>>,
    matcher: SkimMatcherV2,
}

impl ClipboardHistoryPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.clipboard-history".into(),
                name: "Clipboard History".into(),
                description: "Search and restore recently copied text".into(),
                icon: "clipboard".into(),
                keyword: Some("cb".into()),
                keyword_only: false,
            },
            app,
            history: Arc::new(RwLock::new(VecDeque::new())),
            matcher: SkimMatcherV2::default(),
        }
    }

    fn entry_to_result(entry: &ClipEntry, score: f64) -> SearchResult {
        let preview: String = entry
            .text
            .trim()
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .take(PREVIEW_LEN)
            .collect();

        SearchResult {
            id: format!("conduit.clipboard-history:{}", entry.hash_id()),
            plugin_id: "conduit.clipboard-history".into(),
            title: preview,
            subtitle: Some(crate::i18n::tf(
                "plugins",
                "clipboard_subtitle",
                &[
                    ("when", &format_relative_time(entry.copied_at)),
                    ("chars", &entry.text.chars().count().to_string()),
                ],
            )),
            icon: ResultIcon::Named("content_paste".into()),
            score,
            actions: vec![
                Action {
                    id: "copy".into(),
                    title: crate::i18n::t("plugins", "copy_clipboard").into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "delete".into(),
                    title: crate::i18n::t("plugins", "delete_entry").into(),
                    shortcut: None,
                },
            ],
            match_indices: vec![],
        }
    }
}

#[async_trait]
impl ConduitPlugin for ClipboardHistoryPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        // Poll the clipboard in the background and record text changes
        let app = self.app.clone();
        let history = self.history.clone();

        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            loop {
                interval.tick().await;

                // Non-text clipboard contents (images etc.) just error out; skip them
                let Ok(text) = app.clipboard().read_text() else {
                    continue;
                };
                if text.trim().is_empty() {
                    continue;
                }

                let mut history = history.write().await;

                // Unchanged since last poll: nothing to do
                if history.front().map(|e| e.text == text).unwrap_or(false) {
                    continue;
                }

                // Re-copied older entry: move it to the front with a fresh timestamp
                if let Some(pos) = history.iter().position(|e| e.text == text) {
                    history.remove(pos);
                }

                history.push_front(ClipEntry {
                    text,
                    copied_at: SystemTime::now(),
                });
                history.truncate(MAX_ENTRIES);
            }
        });

        Ok(())
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let history = self.history.read().await;
        let query = query.trim();

        if query.is_empty() {
            // Bare "cb" keyword or the default dashboard view: newest first.
            // Scores stay below app favorites (0.9-) and open windows (0.65-)
            // so the empty-query dashboard reads favorites > windows > clips.
            return history
                .iter()
                .take(DEFAULT_LISTING)
                .enumerate()
                .map(|(i, entry)| {
                    Self::entry_to_result(entry, (0.5 - i as f64 * 0.04).max(0.05))
                })
                .collect();
        }

        let mut results: Vec<SearchResult> = history
            .iter()
            .filter_map(|entry| {
                self.matcher.fuzzy_match(&entry.text, query).map(|score| {
                    let normalized = (score as f64 / 100.0).min(0.95).max(0.0);
                    Self::entry_to_result(entry, normalized)
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(DEFAULT_LISTING);
        results
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let id: u64 = result_id
            .strip_prefix("conduit.clipboard-history:")
            .unwrap_or(result_id)
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid clipboard entry id: {}", result_id))?;

        if action_id == "delete" {
            let mut history = self.history.write().await;
            history.retain(|e| e.hash_id() != id);
            return Ok(());
        }

        let history = self.history.read().await;
        let entry = history
            .iter()
            .find(|e| e.hash_id() == id)
            .ok_or_else(|| anyhow::anyhow!("clipboard entry not found"))?;

        self.app
            .clipboard()
            .write_text(entry.text.clone())
            .map_err(|e| anyhow::anyhow!("failed to write clipboard: {}", e))?;
        Ok(())
    }
}

fn format_relative_time(t: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(t)
        .unwrap_or_default()
        .as_secs();
    crate::i18n::relative_time(seconds)
}
