use async_trait::async_trait;

use super::types::{Payload, PluginManifest, SearchResult};

#[async_trait]
pub trait ConduitPlugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    async fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn search(&self, query: &str) -> Vec<SearchResult>;

    /// Enumerate everything this plugin has registered (for browsing UIs,
    /// not search). Plugins with a meaningful catalog override this.
    async fn browse(&self) -> Vec<SearchResult> {
        vec![]
    }

    /// What this result could be handed to another tool as, if anything.
    ///
    /// Answering turns a row into something that can be sent — a copied
    /// line into the character counter, a picked color into the palette.
    /// Plugins whose results are actions rather than values (launching an
    /// app, switching a window) leave this alone.
    async fn payload(&self, _result_id: &str) -> Option<Payload> {
        None
    }

    /// Execute an action on a result. `action_id` is one of the ids from the
    /// result's `actions`; plugins treat unknown ids as the default action.
    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()>;
}
