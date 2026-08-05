use async_trait::async_trait;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

pub struct CalculatorPlugin {
    manifest: PluginManifest,
    app: AppHandle,
}

impl CalculatorPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.calculator".into(),
                name: "Calculator".into(),
                description: "Evaluate math expressions".into(),
                icon: "calculator".into(),
                keyword: Some("=".into()),
                keyword_only: false,
            },
            app,
        }
    }
}

#[async_trait]
impl ConduitPlugin for CalculatorPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let expr = query.trim();
        if expr.is_empty() {
            return vec![];
        }

        // Try to evaluate as math expression
        match meval::eval_str(expr) {
            Ok(value) => {
                let display = if value.fract() == 0.0 && value.abs() < 1e15 {
                    format!("{}", value as i64)
                } else {
                    format!("{}", value)
                };
                vec![SearchResult {
                    id: format!("conduit.calculator:{}", display),
                    plugin_id: "conduit.calculator".into(),
                    title: display,
                    subtitle: Some(format!("= {}", expr)),
                    icon: ResultIcon::Emoji("🧮".into()),
                    score: 1.0,
                    actions: vec![Action {
                        id: "copy".into(),
                        title: "結果をコピー".into(),
                        shortcut: Some("Enter".into()),
                    }],
                    match_indices: vec![],
                }]
            }
            Err(_) => vec![],
        }
    }

    async fn execute(&self, result_id: &str, _action_id: &str) -> anyhow::Result<()> {
        // result_id format: "conduit.calculator:VALUE"
        let value = result_id
            .strip_prefix("conduit.calculator:")
            .unwrap_or(result_id);

        self.app
            .clipboard()
            .write_text(value.to_string())
            .map_err(|e| anyhow::anyhow!("failed to copy to clipboard: {}", e))?;
        Ok(())
    }
}
