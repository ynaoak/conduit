use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    /// Optional trigger prefix, e.g. "=" for calculator
    pub keyword: Option<String>,
    /// Only respond to keyword-routed queries; skip the general fan-out.
    /// Use for expensive searches (e.g. spawning external processes).
    #[serde(default)]
    pub keyword_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub plugin_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: ResultIcon,
    /// 0.0 - 1.0, used for cross-plugin ranking
    pub score: f64,
    pub actions: Vec<Action>,
    /// Char indices of `title` that matched the query (for highlighting)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_indices: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ResultIcon {
    Svg(String),
    Named(String),
    Emoji(String),
    Base64(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub title: String,
    pub shortcut: Option<String>,
}
