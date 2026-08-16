pub mod builtin;
pub mod tool_window;

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
    /// Package version; the built-in package uses it to know when the
    /// installed copy is stale and must be rewritten
    #[serde(default)]
    pub version: Option<String>,
    pub apps: Vec<WebAppDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAppDef {
    pub name: String,
    /// Web app: target URL, may contain a `{query}` placeholder for keyword
    /// search apps. Exactly one of `url` / `html` must be set.
    #[serde(default)]
    pub url: Option<String>,
    /// HTML app: package-relative path to an HTML file opened in its own
    /// window (with its sibling js/css assets served alongside)
    #[serde(default)]
    pub html: Option<String>,
    /// Trigger keyword, e.g. "yt" -> "yt cats" searches YouTube
    #[serde(default)]
    pub keyword: Option<String>,
    /// Emoji, or a Material symbol name (ascii identifiers render as the
    /// symbol when the frontend knows it)
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Opening size of the tool window, in logical pixels. An app that
    /// knows it needs the room (a two-column table, a side-by-side
    /// comparison) says so; everything else gets the default, and a
    /// window the user resized is theirs from then on.
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
}

/// Workflow icon strings are either an emoji or a Material symbol name.
/// Identifier-looking strings go out as Named so the frontend can render
/// the symbol (unknown names degrade to a letter, as before).
fn icon_value(s: String) -> ResultIcon {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        ResultIcon::Named(s)
    } else {
        ResultIcon::Emoji(s)
    }
}

/// Display name for a workflow package. The built-in one is ours, so it
/// follows the UI language; imported packages show their manifest text.
pub fn package_name(manifest: &WorkflowManifest) -> String {
    if builtin::is_builtin(&manifest.id) {
        return crate::i18n::t("tools", "package_name").to_string();
    }
    manifest.name.clone()
}

pub fn package_description(manifest: &WorkflowManifest) -> String {
    if builtin::is_builtin(&manifest.id) {
        return crate::i18n::t("tools", "package_desc").to_string();
    }
    manifest.description.clone()
}

/// App name / description, localized for the built-in package only.
pub fn app_name(manifest: &WorkflowManifest, app: &WebAppDef) -> String {
    localized(manifest, app, "title").unwrap_or_else(|| app.name.clone())
}

pub fn app_description(manifest: &WorkflowManifest, app: &WebAppDef) -> Option<String> {
    localized(manifest, app, "desc").or_else(|| app.description.clone())
}

fn localized(manifest: &WorkflowManifest, app: &WebAppDef, suffix: &str) -> Option<String> {
    if !builtin::is_builtin(&manifest.id) {
        return None;
    }
    let key = builtin::locale_key(app.html.as_deref()?)?;
    Some(crate::i18n::t("tools", &format!("{}_{}", key, suffix)).to_string())
}

/// Result id for a web app (the launcher opens the URL in the browser).
/// Search results and the manage-view listing must build identical ids,
/// since both are fed back into `execute`.
pub fn web_result_id(url: &str) -> String {
    format!("conduit.workflows:{}", url)
}

/// Result id for an HTML app (opened in its own tool window)
pub fn html_result_id(workflow_id: &str, html: &str) -> String {
    format!("conduit.workflows:html:{}/{}", workflow_id, html)
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
            id: web_result_id(&url),
            plugin_id: "conduit.workflows".into(),
            title,
            subtitle: Some(
                app_description(workflow, app)
                    .unwrap_or_else(|| {
                        crate::i18n::tf(
                            "plugins",
                            "workflow_open_browser",
                            &[("name", &workflow.name)],
                        )
                    }),
            ),
            icon: match app.icon.clone().or_else(|| workflow.icon.clone()) {
                Some(value) => icon_value(value),
                None => ResultIcon::Named("language".into()),
            },
            score,
            actions: vec![
                Action {
                    id: "open".into(),
                    title: crate::i18n::t("plugins", "open").into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "copy-url".into(),
                    title: crate::i18n::t("plugins", "copy_url").into(),
                    shortcut: None,
                },
            ],
            match_indices: vec![],
        }
    }
}

impl WorkflowsPlugin {
    fn html_app_to_result(
        workflow: &WorkflowManifest,
        app: &WebAppDef,
        html: &str,
        score: f64,
    ) -> SearchResult {
        SearchResult {
            id: html_result_id(&workflow.id, html),
            plugin_id: "conduit.workflows".into(),
            title: app_name(workflow, app),
            subtitle: Some(
                app_description(workflow, app)
                    .unwrap_or_else(|| {
                        crate::i18n::tf(
                            "plugins",
                            "workflow_open_window",
                            &[("name", &workflow.name)],
                        )
                    }),
            ),
            icon: match app.icon.clone().or_else(|| workflow.icon.clone()) {
                Some(value) => icon_value(value),
                None => ResultIcon::Named("select_window".into()),
            },
            score,
            actions: vec![Action {
                id: "open".into(),
                title: crate::i18n::t("plugins", "open").into(),
                shortcut: Some("Enter".into()),
            }],
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
        let disabled = self
            .app
            .state::<crate::config::ConfigState>()
            .get()
            .await
            .workflows
            .disabled;
        let mut results = Vec::new();

        for workflow in &workflows {
            if disabled.contains(&workflow.id) {
                continue;
            }
            for app in &workflow.apps {
                // Keyword match. Web apps substitute the rest into {query};
                // HTML apps just open (there is nothing to substitute into).
                if let Some(ref kw) = app.keyword {
                    let rest = match query.strip_prefix(kw.as_str()) {
                        Some("") => Some(""),
                        Some(r) if r.starts_with(char::is_whitespace) => Some(r.trim_start()),
                        _ => None,
                    };
                    if let Some(rest) = rest {
                        if let Some(ref url) = app.url {
                            let url = url.replace("{query}", &urlencoding::encode(rest));
                            let name = app_name(workflow, app);
                            let title = if rest.is_empty() {
                                name
                            } else {
                                format!("{}: {}", name, rest)
                            };
                            results.push(Self::app_to_result(workflow, app, url, 1.0, title));
                        } else if let Some(ref html) = app.html {
                            results.push(Self::html_app_to_result(workflow, app, html, 1.0));
                        }
                        continue;
                    }
                }

                // Name fuzzy match. Both the manifest name and the
                // localized one match, so "base64" still finds a built-in
                // app whose displayed name is translated.
                let display_name = app_name(workflow, app);
                let score = self
                    .matcher
                    .fuzzy_match(&display_name, query)
                    .into_iter()
                    .chain(self.matcher.fuzzy_match(&app.name, query))
                    .max();
                if let Some(score) = score {
                    let normalized = (score as f64 / 100.0).min(0.85).max(0.0);
                    if let Some(ref url) = app.url {
                        let url = url.replace("{query}", "");
                        results.push(Self::app_to_result(
                            workflow,
                            app,
                            url,
                            normalized,
                            display_name,
                        ));
                    } else if let Some(ref html) = app.html {
                        results.push(Self::html_app_to_result(workflow, app, html, normalized));
                    }
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

        // HTML app: open in its own window instead of the browser
        if let Some(spec) = url.strip_prefix("html:") {
            let (workflow_id, html) = spec
                .split_once('/')
                .ok_or_else(|| anyhow::anyhow!("malformed html app id: {}", result_id))?;
            return tool_window::open(&self.app, workflow_id, html).await;
        }

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
