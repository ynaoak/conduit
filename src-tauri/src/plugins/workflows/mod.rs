use std::path::PathBuf;

use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

/// A workflow package: a set of web apps installed from a zip.
/// Lives at `{app_config_dir}/workflows/<id>/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub apps: Vec<WebAppDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAppDef {
    pub name: String,
    /// Target URL. May contain a `{query}` placeholder for keyword search apps.
    pub url: String,
    /// Trigger keyword, e.g. "yt" -> "yt cats" searches YouTube
    #[serde(default)]
    pub keyword: Option<String>,
    /// Emoji icon
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Directory holding installed workflow packages
pub fn workflows_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join("workflows"))
}

/// Load all installed workflow manifests. Invalid manifests are skipped.
pub fn load_workflows(dir: &PathBuf) -> Vec<WorkflowManifest> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut workflows: Vec<WorkflowManifest> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let manifest_path = e.path().join("manifest.json");
            let contents = std::fs::read_to_string(manifest_path).ok()?;
            serde_json::from_str::<WorkflowManifest>(&contents).ok()
        })
        .collect();

    workflows.sort_by(|a, b| a.name.cmp(&b.name));
    workflows
}

pub struct WorkflowsPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    dir: Option<PathBuf>,
    matcher: SkimMatcherV2,
}

impl WorkflowsPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.workflows".into(),
                name: "Workflows".into(),
                description: "Web apps imported from workflow packages".into(),
                icon: "workflow".into(),
                keyword: None,
                keyword_only: false,
            },
            dir: workflows_dir(&app).ok(),
            app,
            matcher: SkimMatcherV2::default(),
        }
    }

    fn app_to_result(workflow: &WorkflowManifest, app: &WebAppDef, url: String, score: f64, title: String) -> SearchResult {
        SearchResult {
            id: format!("conduit.workflows:{}", url),
            plugin_id: "conduit.workflows".into(),
            title,
            subtitle: Some(
                app.description
                    .clone()
                    .unwrap_or_else(|| format!("{} ・ ブラウザで開く", workflow.name)),
            ),
            icon: ResultIcon::Emoji(
                app.icon
                    .clone()
                    .or_else(|| workflow.icon.clone())
                    .unwrap_or_else(|| "🌐".into()),
            ),
            score,
            actions: vec![
                Action {
                    id: "open".into(),
                    title: "開く".into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "copy-url".into(),
                    title: "URL をコピー".into(),
                    shortcut: None,
                },
            ],
            match_indices: vec![],
        }
    }
}

#[async_trait]
impl ConduitPlugin for WorkflowsPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return vec![];
        }
        let Some(ref dir) = self.dir else {
            return vec![];
        };

        // Manifests are tiny and few; reloading per search keeps imports
        // visible immediately without any reload coordination.
        let workflows = load_workflows(dir);
        let mut results = Vec::new();

        for workflow in &workflows {
            for app in &workflow.apps {
                // Keyword search app: "yt cats" -> substituted URL
                if let Some(ref kw) = app.keyword {
                    let rest = match query.strip_prefix(kw.as_str()) {
                        Some("") => Some(""),
                        Some(r) if r.starts_with(char::is_whitespace) => Some(r.trim_start()),
                        _ => None,
                    };
                    if let Some(rest) = rest {
                        let url = app
                            .url
                            .replace("{query}", &urlencoding::encode(rest));
                        let title = if rest.is_empty() {
                            app.name.clone()
                        } else {
                            format!("{}: {}", app.name, rest)
                        };
                        results.push(Self::app_to_result(workflow, app, url, 1.0, title));
                        continue;
                    }
                }

                // Name fuzzy match
                if let Some(score) = self.matcher.fuzzy_match(&app.name, query) {
                    let normalized = (score as f64 / 100.0).min(0.85).max(0.0);
                    let url = app.url.replace("{query}", "");
                    results.push(Self::app_to_result(
                        workflow,
                        app,
                        url,
                        normalized,
                        app.name.clone(),
                    ));
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(10);
        results
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let url = result_id
            .strip_prefix("conduit.workflows:")
            .unwrap_or(result_id);

        if !url.starts_with("http://") && !url.starts_with("https://") {
            anyhow::bail!("unsupported URL scheme: {}", url);
        }

        if action_id == "copy-url" {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            self.app
                .clipboard()
                .write_text(url.to_string())
                .map_err(|e| anyhow::anyhow!("failed to copy URL: {}", e))?;
            return Ok(());
        }

        // opener handles cmd metacharacters (&, etc.) in URLs safely
        use tauri_plugin_opener::OpenerExt;
        self.app.opener().open_url(url, None::<&str>)?;
        Ok(())
    }
}
