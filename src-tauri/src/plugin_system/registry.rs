use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::traits::ConduitPlugin;
use super::types::SearchResult;
use crate::config::ConfigState;

#[derive(Clone)]
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Box<dyn ConduitPlugin>>>>,
    config: ConfigState,
}

impl PluginRegistry {
    pub fn new(config: ConfigState) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub async fn register(&self, mut plugin: Box<dyn ConduitPlugin>) {
        let id = plugin.manifest().id.clone();
        let _ = plugin.init().await;
        self.plugins.write().await.insert(id, plugin);
    }

    /// Check if a query matches a plugin keyword.
    /// Returns the remaining query (trimmed) if it matches.
    ///
    /// Symbolic keywords (e.g. "=") match as a plain prefix ("=1+2").
    /// Word keywords (e.g. "cb") require an exact match or a following
    /// whitespace ("cb foo") so that general queries like "cbc news"
    /// are not hijacked.
    fn keyword_match(keyword: &str, query: &str) -> Option<String> {
        let rest = query.strip_prefix(keyword)?;
        let symbolic = keyword
            .chars()
            .last()
            .map_or(false, |c| !c.is_alphanumeric());
        if symbolic || rest.is_empty() || rest.starts_with(char::is_whitespace) {
            Some(rest.trim().to_string())
        } else {
            None
        }
    }

    pub async fn search(&self, query: &str) -> Vec<SearchResult> {
        let config = self.config.get().await;
        let plugins = self.plugins.read().await;

        // Keyword routing: if query starts with a plugin keyword, route only to that plugin.
        // An empty remainder is passed through so plugins can show a default listing
        // (e.g. clipboard history shows recent entries for the bare "cb" keyword).
        for plugin in plugins.values() {
            let id = &plugin.manifest().id;
            if !config.plugins.enabled.get(id).copied().unwrap_or(true) {
                continue;
            }
            if let Some(ref kw) = plugin.manifest().keyword {
                if let Some(stripped) = Self::keyword_match(kw, query) {
                    return plugin.search(&stripped).await;
                }
            }
        }

        // General search: fan out to all enabled plugins
        let mut all_results = Vec::new();
        for plugin in plugins.values() {
            let id = &plugin.manifest().id;
            if !config.plugins.enabled.get(id).copied().unwrap_or(true) {
                continue;
            }
            if plugin.manifest().keyword_only {
                continue;
            }
            let mut results = plugin.search(query).await;
            all_results.append(&mut results);
        }

        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_results.truncate(config.search.max_results);
        all_results
    }

    pub async fn browse(&self, plugin_id: &str) -> Vec<SearchResult> {
        let plugins = self.plugins.read().await;
        match plugins.get(plugin_id) {
            Some(plugin) => plugin.browse().await,
            None => vec![],
        }
    }

    pub async fn execute(
        &self,
        plugin_id: &str,
        result_id: &str,
        action_id: &str,
    ) -> anyhow::Result<()> {
        let plugins = self.plugins.read().await;
        if let Some(plugin) = plugins.get(plugin_id) {
            plugin.execute(result_id, action_id).await
        } else {
            Err(anyhow::anyhow!("Plugin not found: {}", plugin_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PluginRegistry as R;

    #[test]
    fn word_keyword_needs_a_boundary() {
        // Bare keyword lists the default view, "ps <arg>" passes the argument
        assert_eq!(R::keyword_match("ps", "ps"), Some(String::new()));
        assert_eq!(R::keyword_match("ps", "ps 3000"), Some("3000".into()));
        assert_eq!(R::keyword_match("ps", "ps  node "), Some("node".into()));
        // Words that merely start with the keyword must not be hijacked
        assert_eq!(R::keyword_match("ps", "psql"), None);
        assert_eq!(R::keyword_match("ps", "photoshop"), None);
    }

    #[test]
    fn symbolic_keyword_matches_as_a_plain_prefix() {
        assert_eq!(R::keyword_match("=", "=1+2"), Some("1+2".into()));
    }
}
