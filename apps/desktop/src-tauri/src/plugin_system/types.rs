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

/// Something a result can hand to a tool window.
///
/// Not a field on `SearchResult`: a payload is derived from the result on
/// demand (see `ConduitPlugin::payload`), so it never has to cross the IPC
/// and every plugin that has nothing to send says nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    /// What kind of thing this is — matched against an app's `accepts`.
    /// "text" and "color" today; a package may invent its own.
    pub kind: String,
    pub value: String,
}

impl Payload {
    pub fn text(value: impl Into<String>) -> Self {
        Self { kind: "text".into(), value: value.into() }
    }

    pub fn color(value: impl Into<String>) -> Self {
        Self { kind: "color".into(), value: value.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub title: String,
    pub shortcut: Option<String>,
}

/// A registered plugin as the settings tab sees it: the manifest said in
/// the UI language, plus the on/off state that tab edits.
///
/// Separate from `PluginManifest` because the two are written for
/// different readers. A manifest is source: its `name` and `description`
/// sit next to the code, in English, and nothing shows them to anyone.
/// This is a listing, so the wording comes from `locales/` like every
/// other string a user reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub keyword: Option<String>,
    pub keyword_only: bool,
    pub enabled: bool,
}
