use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

struct SystemCommand {
    name: &'static str,
    description: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    emoji: &'static str,
}

const COMMANDS: &[SystemCommand] = &[
    SystemCommand {
        name: "シャットダウン",
        description: "PC をシャットダウン",
        command: "shutdown",
        args: &["/s", "/t", "0"],
        emoji: "⏻",
    },
    SystemCommand {
        name: "再起動",
        description: "PC を再起動",
        command: "shutdown",
        args: &["/r", "/t", "0"],
        emoji: "🔄",
    },
    SystemCommand {
        name: "ロック",
        description: "画面をロック",
        command: "rundll32.exe",
        args: &["user32.dll,LockWorkStation"],
        emoji: "🔒",
    },
    SystemCommand {
        name: "スリープ",
        description: "PC をスリープ",
        command: "rundll32.exe",
        args: &["powrprof.dll,SetSuspendState", "0,1,0"],
        emoji: "😴",
    },
    SystemCommand {
        name: "サインアウト",
        description: "Windows からサインアウト",
        command: "shutdown",
        args: &["/l"],
        emoji: "🚪",
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
                let name_match = self.matcher.fuzzy_indices(cmd.name, query);
                let desc_score = self.matcher.fuzzy_match(cmd.description, query);
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
                        id: format!("conduit.system-commands:{}", cmd.name),
                        plugin_id: "conduit.system-commands".into(),
                        title: cmd.name.to_string(),
                        subtitle: Some(cmd.description.to_string()),
                        icon: ResultIcon::Emoji(cmd.emoji.into()),
                        score: normalized,
                        actions: vec![Action {
                            id: "execute".into(),
                            title: "実行".into(),
                            shortcut: Some("Enter".into()),
                        }],
                        match_indices,
                    }
                })
            })
            .collect()
    }

    async fn execute(&self, result_id: &str, _action_id: &str) -> anyhow::Result<()> {
        let name = result_id
            .strip_prefix("conduit.system-commands:")
            .unwrap_or(result_id);

        if let Some(cmd) = COMMANDS.iter().find(|c| c.name == name) {
            std::process::Command::new(cmd.command)
                .args(cmd.args)
                .spawn()?;
        }

        Ok(())
    }
}
