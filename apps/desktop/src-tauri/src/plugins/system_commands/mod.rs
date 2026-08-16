use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

struct SystemCommand {
    /// Locale key: `plugins.system_<key>` / `plugins.system_<key>_desc`
    key: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    icon: &'static str,
}

const COMMANDS: &[SystemCommand] = &[
    SystemCommand {
        key: "shutdown",
        command: "shutdown",
        args: &["/s", "/t", "0"],
        icon: "power_settings_new",
    },
    SystemCommand {
        key: "restart",
        command: "shutdown",
        args: &["/r", "/t", "0"],
        icon: "restart_alt",
    },
    SystemCommand {
        key: "lock",
        command: "rundll32.exe",
        args: &["user32.dll,LockWorkStation"],
        icon: "lock",
    },
    SystemCommand {
        key: "sleep",
        command: "rundll32.exe",
        args: &["powrprof.dll,SetSuspendState", "0,1,0"],
        icon: "bedtime",
    },
    SystemCommand {
        key: "signout",
        command: "shutdown",
        args: &["/l"],
        icon: "logout",
    },
];

pub struct SystemCommandsPlugin {
    manifest: PluginManifest,
    matcher: SkimMatcherV2,
}

impl SystemCommandsPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest {
                id: "conduit.system-commands".into(),
                name: "System Commands".into(),
                description: "System actions like shutdown, restart, lock".into(),
                icon: "system".into(),
                keyword: None,
                keyword_only: false,
            },
            matcher: SkimMatcherV2::default(),
        }
    }
}

#[async_trait]
impl ConduitPlugin for SystemCommandsPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        if query.trim().is_empty() {
            return vec![];
        }

        COMMANDS
            .iter()
            .filter_map(|cmd| {
                let name = crate::i18n::t("plugins", &format!("system_{}", cmd.key));
                let description =
                    crate::i18n::t("plugins", &format!("system_{}_desc", cmd.key));
                let name_match = self.matcher.fuzzy_indices(name, query);
                let desc_score = self.matcher.fuzzy_match(description, query);
                let name_score = name_match.as_ref().map(|(s, _)| *s);
                let best_score = name_score.max(desc_score);

                best_score.map(|score| {
                    let normalized = (score as f64 / 100.0).min(0.9).max(0.0);
                    // Highlight only when the name itself was the better match
                    let match_indices = match name_match {
                        Some((s, indices)) if Some(s) >= desc_score => indices,
                        _ => vec![],
                    };
                    SearchResult {
                        // The id keys off the locale-independent key, so a
                        // pinned command survives a language change
                        id: format!("conduit.system-commands:{}", cmd.key),
                        plugin_id: "conduit.system-commands".into(),
                        title: name.to_string(),
                        subtitle: Some(description.to_string()),
                        icon: ResultIcon::Named(cmd.icon.into()),
                        score: normalized,
                        actions: vec![Action {
                            id: "execute".into(),
                            title: crate::i18n::t("plugins", "run").into(),
                            shortcut: Some("Enter".into()),
                        }],
                        match_indices,
                    }
                })
            })
            .collect()
    }

    async fn execute(&self, result_id: &str, _action_id: &str) -> anyhow::Result<()> {
        let key = result_id
            .strip_prefix("conduit.system-commands:")
            .unwrap_or(result_id);

        if let Some(cmd) = COMMANDS.iter().find(|c| c.key == key) {
            std::process::Command::new(cmd.command)
                .args(cmd.args)
                .spawn()?;
        }

        Ok(())
    }
}
