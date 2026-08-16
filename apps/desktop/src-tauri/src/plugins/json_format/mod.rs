use async_trait::async_trait;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

const PLUGIN_ID: &str = "conduit.json";
/// Title preview length (chars). The full value lives in the result id, so
/// truncation only affects what the row shows.
const PREVIEW_LEN: usize = 80;
/// Result id of the "this is not JSON" row — it carries an error message,
/// not a value, so `execute` has nothing to copy for it.
const INVALID_ID: &str = "conduit.json:invalid";

/// Which rendering produced a result. Kept in the result id so the two rows
/// never collide (and so pins survive a restart).
#[derive(Clone, Copy)]
enum Shape {
    Pretty,
    Minify,
}

impl Shape {
    fn id(self) -> &'static str {
        match self {
            Shape::Pretty => "pretty",
            Shape::Minify => "minify",
        }
    }
}

#[derive(Debug)]
struct Rendered {
    pretty: String,
    minified: String,
}

/// Parse `input` as JSON and render it both ways.
///
/// Object keys keep their original order and numbers keep their original
/// spelling (`serde_json`'s `preserve_order` / `arbitrary_precision`): a
/// formatter that silently re-sorted keys or rounded a 20-digit id would
/// corrupt the document it was asked to tidy up.
fn render(input: &str) -> Result<Rendered, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    Ok(Rendered {
        pretty: serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?,
        minified: serde_json::to_string(&value).map_err(|e| e.to_string())?,
    })
}

/// Input that already looks indented is one someone wants collapsed, so the
/// minified row leads. Two consecutive spaces stand in for indentation:
/// pasting into a single-line input can eat the newlines, but not the indent.
fn leads_with_minify(input: &str) -> bool {
    input.contains('\n') || input.contains("  ")
}

fn preview(text: &str) -> String {
    let flattened: String = text
        .trim()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(PREVIEW_LEN)
        .collect();

    if text.trim().chars().count() > PREVIEW_LEN {
        format!("{}…", flattened)
    } else {
        flattened
    }
}

pub struct JsonFormatPlugin {
    manifest: PluginManifest,
    app: AppHandle,
}

impl JsonFormatPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: PLUGIN_ID.into(),
                name: "JSON".into(),
                description: "Pretty-print and minify JSON".into(),
                icon: "json".into(),
                keyword: Some("json".into()),
                // Bare numbers and quoted words are valid JSON, so joining the
                // general fan-out would answer queries like "123" with a JSON
                // row — require the keyword.
                keyword_only: true,
            },
            app,
        }
    }

    fn to_result(shape: Shape, value: String, score: f64) -> SearchResult {
        let chars = value.chars().count().to_string();
        let subtitle = match shape {
            Shape::Pretty => crate::i18n::tf(
                "plugins",
                "json_pretty",
                &[("lines", &value.lines().count().to_string()), ("chars", &chars)],
            ),
            Shape::Minify => crate::i18n::tf("plugins", "json_minify", &[("chars", &chars)]),
        };

        SearchResult {
            id: format!("{}:{}:{}", PLUGIN_ID, shape.id(), value),
            plugin_id: PLUGIN_ID.into(),
            title: preview(&value),
            subtitle: Some(subtitle),
            icon: ResultIcon::Named("data_object".into()),
            score,
            actions: vec![Action {
                id: "copy".into(),
                title: crate::i18n::t("plugins", "copy_result").into(),
                shortcut: Some("Enter".into()),
            }],
            match_indices: vec![],
        }
    }

    /// The parse error is the useful answer when the input is broken —
    /// serde_json reports the line and column of the offending token.
    fn to_error_result(message: String) -> SearchResult {
        SearchResult {
            id: INVALID_ID.into(),
            plugin_id: PLUGIN_ID.into(),
            title: crate::i18n::t("plugins", "json_invalid").into(),
            subtitle: Some(message),
            icon: ResultIcon::Named("warning".into()),
            score: 1.0,
            actions: vec![],
            match_indices: vec![],
        }
    }
}

#[async_trait]
impl ConduitPlugin for JsonFormatPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let input = query.trim();
        if input.is_empty() {
            return vec![];
        }

        let rendered = match render(input) {
            Ok(rendered) => rendered,
            Err(message) => return vec![Self::to_error_result(message)],
        };

        // Scalars and empty containers render identically both ways — one row
        if rendered.pretty == rendered.minified {
            return vec![Self::to_result(Shape::Pretty, rendered.pretty, 1.0)];
        }

        if leads_with_minify(input) {
            vec![
                Self::to_result(Shape::Minify, rendered.minified, 1.0),
                Self::to_result(Shape::Pretty, rendered.pretty, 0.9),
            ]
        } else {
            vec![
                Self::to_result(Shape::Pretty, rendered.pretty, 1.0),
                Self::to_result(Shape::Minify, rendered.minified, 0.9),
            ]
        }
    }

    async fn execute(&self, result_id: &str, _action_id: &str) -> anyhow::Result<()> {
        // The error row has no value behind it; Enter on it is a no-op
        if result_id == INVALID_ID {
            return Ok(());
        }

        // result_id format: "conduit.json:<shape>:<value>". The value is the
        // whole tail, so it may itself contain ':' or newlines.
        let value = result_id
            .strip_prefix(&format!("{}:", PLUGIN_ID))
            .and_then(|rest| rest.split_once(':'))
            .map(|(_, value)| value)
            .ok_or_else(|| anyhow::anyhow!("malformed json result id: {}", result_id))?;

        self.app
            .clipboard()
            .write_text(value.to_string())
            .map_err(|e| anyhow::anyhow!("failed to copy to clipboard: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_compact_json_with_two_space_indent() {
        let out = render(r#"{"a":1,"b":[1,2]}"#).unwrap();
        assert_eq!(
            out.pretty,
            "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}"
        );
    }

    #[test]
    fn collapses_indented_json_to_one_line() {
        let out = render("{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}").unwrap();
        assert_eq!(out.minified, r#"{"a":1,"b":[1,2]}"#);
    }

    #[test]
    fn keeps_the_original_key_order() {
        let out = render(r#"{"z":1,"a":2,"m":3}"#).unwrap();
        assert_eq!(out.minified, r#"{"z":1,"a":2,"m":3}"#);
    }

    #[test]
    fn keeps_numbers_exactly_as_written() {
        // Both would be mangled by a round trip through f64
        let out = render(r#"{"id":12345678901234567890,"rate":0.1234567890123456789}"#).unwrap();
        assert_eq!(
            out.minified,
            r#"{"id":12345678901234567890,"rate":0.1234567890123456789}"#
        );
    }

    #[test]
    fn keeps_non_ascii_text_unescaped() {
        let out = render(r#"{"名前":"日本語 🧾"}"#).unwrap();
        assert_eq!(out.minified, r#"{"名前":"日本語 🧾"}"#);
    }

    #[test]
    fn scalars_render_the_same_both_ways() {
        for input in ["123", "\"text\"", "true", "null", "[]", "{}"] {
            let out = render(input).unwrap();
            assert_eq!(out.pretty, out.minified, "input: {}", input);
        }
    }

    #[test]
    fn reports_where_the_parse_failed() {
        let err = render(r#"{"a": 1,}"#).unwrap_err();
        assert!(err.contains("line 1"), "unexpected message: {}", err);
        assert!(render("hello").is_err());
        assert!(render("{").is_err());
    }

    #[test]
    fn indented_input_leads_with_minify() {
        assert!(leads_with_minify("{\n  \"a\": 1\n}"));
        assert!(leads_with_minify(r#"{  "a": 1 }"#));
        assert!(!leads_with_minify(r#"{"a":1,"b":2}"#));
        assert!(!leads_with_minify(r#"{"a": 1, "b": 2}"#));
    }
}
