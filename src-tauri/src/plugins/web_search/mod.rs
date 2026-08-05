use async_trait::async_trait;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::config::{self, ConfigState};
use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

pub struct WebSearchPlugin {
    manifest: PluginManifest,
    config: ConfigState,
    app: AppHandle,
}

impl WebSearchPlugin {
    pub fn new(config: ConfigState, app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.web-search".into(),
                name: "Web Search".into(),
                description: "Search the web".into(),
                icon: "search".into(),
                keyword: None,
                keyword_only: false,
            },
            config,
            app,
        }
    }

    fn engine_display_name(engine: &str) -> &str {
        match engine.to_lowercase().as_str() {
            "google" => "Google",
            "duckduckgo" | "ddg" => "DuckDuckGo",
            "bing" => "Bing",
            _ => "Web",
        }
    }
}

#[async_trait]
impl ConduitPlugin for WebSearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return vec![];
        }

        let cfg = self.config.get().await;
        let engine_name = Self::engine_display_name(&cfg.search.web_search_engine);

        vec![SearchResult {
            id: format!("conduit.web-search:{}", query),
            plugin_id: "conduit.web-search".into(),
            title: format!("{} で「{}」を検索", engine_name, query),
            subtitle: Some("ブラウザで開く".into()),
            icon: ResultIcon::Emoji("\u{1F50D}".into()),
            score: 0.2,
            actions: vec![
                Action {
                    id: "search".into(),
                    title: "検索".into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "copy-url".into(),
                    title: "URL をコピー".into(),
                    shortcut: None,
                },
            ],
            match_indices: vec![],
        }]
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let query = result_id
            .strip_prefix("conduit.web-search:")
            .unwrap_or(result_id);

        let cfg = self.config.get().await;
        let url = config::resolve_search_url(&cfg.search.web_search_engine, query);

        if action_id == "copy-url" {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            self.app
                .clipboard()
                .write_text(url)
                .map_err(|e| anyhow::anyhow!("failed to copy URL: {}", e))?;
            return Ok(());
        }

        // opener handles cmd metacharacters (&, etc.) in URLs safely
        self.app.opener().open_url(&url, None::<&str>)?;

        Ok(())
    }
}
