use tauri::{AppHandle, State};

use crate::config::ConfigState;
use crate::pins::{PinnedItem, PinsState};
use crate::plugin_system::registry::PluginRegistry;
use crate::plugin_system::types::SearchResult;

/// Pins sit above every dashboard section (apps 0.9-), so any score over
/// that works; the order among pins is their position, not this value.
const PIN_SCORE: f64 = 1.0;

/// Put the pinned favorites ahead of `results`, in pinned order.
///
/// A result that is pinned is dropped from `results` so it is not listed
/// twice, and the combined list is cut to `max` (pins first, so they are
/// the last thing to be truncated away).
fn with_pins_first(
    pins: Vec<PinnedItem>,
    results: Vec<SearchResult>,
    max: usize,
) -> Vec<SearchResult> {
    let is_pinned = |r: &SearchResult| {
        pins.iter()
            .any(|p| p.plugin_id == r.plugin_id && p.result_id == r.id)
    };
    let rest: Vec<SearchResult> = results.into_iter().filter(|r| !is_pinned(r)).collect();
    let mut merged: Vec<SearchResult> = pins
        .into_iter()
        .map(|p| SearchResult {
            id: p.result_id,
            plugin_id: p.plugin_id,
            title: p.title,
            subtitle: p.subtitle,
            icon: p.icon,
            score: PIN_SCORE,
            actions: p.actions,
            match_indices: Vec::new(),
        })
        .collect();
    merged.extend(rest);
    merged.truncate(max);
    merged
}

#[tauri::command]
pub async fn query(
    query: String,
    app: AppHandle,
    registry: State<'_, PluginRegistry>,
    pins: State<'_, PinsState>,
    config: State<'_, ConfigState>,
) -> Result<Vec<SearchResult>, String> {
    let mut results = registry.search(&query).await;
    // The summoned (empty-query) view opens with the user's favorites, in
    // the order they arranged them in the Pins tab.
    if query.trim().is_empty() {
        let max = config.get().await.search.max_results;
        results = with_pins_first(pins.list().await, results, max);
    }
    // Destinations are decided here rather than inside each plugin: a
    // plugin knows what its result *is*, not what else happens to be
    // installed that could take it.
    for result in &mut results {
        if let Some(payload) = registry.payload(&result.plugin_id, &result.id).await {
            crate::send_to::decorate(&app, result, &payload);
        }
    }
    Ok(results)
}

/// Enumerate a plugin's registered entries (browsing UI, not search)
#[tauri::command]
pub async fn browse_plugin(
    plugin_id: String,
    registry: State<'_, PluginRegistry>,
) -> Result<Vec<SearchResult>, String> {
    Ok(registry.browse(&plugin_id).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_system::types::ResultIcon;

    fn pin(plugin_id: &str, id: &str) -> PinnedItem {
        PinnedItem {
            plugin_id: plugin_id.into(),
            result_id: id.into(),
            title: id.into(),
            subtitle: None,
            icon: ResultIcon::Emoji("📌".into()),
            actions: vec![],
        }
    }

    fn result(plugin_id: &str, id: &str, score: f64) -> SearchResult {
        SearchResult {
            id: id.into(),
            plugin_id: plugin_id.into(),
            title: id.into(),
            subtitle: None,
            icon: ResultIcon::Emoji("🔹".into()),
            score,
            actions: vec![],
            match_indices: vec![],
        }
    }

    fn ids(results: &[SearchResult]) -> Vec<&str> {
        results.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn pins_come_first_in_pinned_order() {
        let merged = with_pins_first(
            vec![pin("a", "p2"), pin("a", "p1")],
            vec![result("b", "x", 0.9), result("b", "y", 0.5)],
            10,
        );
        assert_eq!(ids(&merged), ["p2", "p1", "x", "y"]);
        assert!(merged[0].score > 0.9);
    }

    #[test]
    fn a_pinned_result_is_not_listed_twice() {
        let merged = with_pins_first(
            vec![pin("apps", "calc")],
            vec![result("apps", "calc", 0.95), result("clip", "calc", 0.5)],
            10,
        );
        // Same id from another plugin is a different result and stays.
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].plugin_id, "apps");
        assert_eq!(merged[1].plugin_id, "clip");
    }

    #[test]
    fn truncates_to_max_keeping_pins() {
        let merged = with_pins_first(
            vec![pin("a", "p1"), pin("a", "p2")],
            vec![result("b", "x", 0.9), result("b", "y", 0.5)],
            3,
        );
        assert_eq!(ids(&merged), ["p1", "p2", "x"]);
    }
}
