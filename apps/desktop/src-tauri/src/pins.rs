//! Pinned items (CLaunch-style favorites).
//!
//! A pin stores everything needed to rebuild the original SearchResult, so
//! running one just replays the existing execute_action path — plugins need
//! no knowledge of pinning at all.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::plugin_system::types::{Action, ResultIcon};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedItem {
    pub plugin_id: String,
    pub result_id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    pub icon: ResultIcon,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Clone)]
pub struct PinsState {
    pins: Arc<RwLock<Vec<PinnedItem>>>,
    path: PathBuf,
}

impl PinsState {
    pub fn load(config_dir: &PathBuf) -> Self {
        let path = config_dir.join("pins.json");
        let pins = std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<Vec<PinnedItem>>(&contents).ok())
            .unwrap_or_default();

        Self {
            pins: Arc::new(RwLock::new(pins)),
            path,
        }
    }

    pub async fn list(&self) -> Vec<PinnedItem> {
        self.pins.read().await.clone()
    }

    /// Add a pin (no-op when the same result is already pinned)
    pub async fn add(&self, item: PinnedItem) -> anyhow::Result<()> {
        {
            let mut pins = self.pins.write().await;
            if pins.iter().any(|p| p.result_id == item.result_id) {
                return Ok(());
            }
            pins.push(item);
        }
        self.persist().await
    }

    pub async fn remove(&self, result_id: &str) -> anyhow::Result<()> {
        self.pins.write().await.retain(|p| p.result_id != result_id);
        self.persist().await
    }

    /// Move a pin within the list; out-of-range targets are clamped
    pub async fn move_to(&self, result_id: &str, new_index: usize) -> anyhow::Result<()> {
        {
            let mut pins = self.pins.write().await;
            let Some(current) = pins.iter().position(|p| p.result_id == result_id) else {
                return Ok(());
            };
            let item = pins.remove(current);
            let target = new_index.min(pins.len());
            pins.insert(target, item);
        }
        self.persist().await
    }

    async fn persist(&self) -> anyhow::Result<()> {
        let pins = self.pins.read().await;
        let json = serde_json::to_string_pretty(&*pins)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ResultIcon` is an adjacently tagged enum, which serde buffers while
    /// deserializing — the shape most sensitive to `serde_json`'s
    /// `arbitrary_precision` feature. pins.json is written by one run and
    /// read by the next, so a break here would silently drop every pin.
    #[test]
    fn pins_survive_a_json_round_trip() {
        let pins = vec![PinnedItem {
            plugin_id: "conduit.json".into(),
            result_id: "conduit.json:minify:{\"a\":1}".into(),
            title: "{\"a\":1}".into(),
            subtitle: Some("JSON を圧縮 ・ 7 文字".into()),
            icon: ResultIcon::Emoji("🧾".into()),
            actions: vec![Action {
                id: "copy".into(),
                title: "結果をコピー".into(),
                shortcut: Some("Enter".into()),
            }],
        }];

        let json = serde_json::to_string_pretty(&pins).unwrap();
        let parsed: Vec<PinnedItem> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].result_id, pins[0].result_id);
        assert!(matches!(&parsed[0].icon, ResultIcon::Emoji(e) if e == "🧾"));
        assert_eq!(parsed[0].actions[0].id, "copy");
    }
}
