use async_trait::async_trait;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

const PLUGIN_ID: &str = "conduit.base64";
/// Title preview length (chars). The full value lives in the result id, so
/// truncation only affects what the row shows.
const PREVIEW_LEN: usize = 80;

/// Which direction produced a result. Kept in the result id so encode and
/// decode rows never collide (and so pins survive a restart).
#[derive(Clone, Copy)]
enum Direction {
    Encode,
    Decode,
}

impl Direction {
    fn id(self) -> &'static str {
        match self {
            Direction::Encode => "encode",
            Direction::Decode => "decode",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Direction::Encode => crate::i18n::t("plugins", "base64_encode"),
            Direction::Decode => crate::i18n::t("plugins", "base64_decode"),
        }
    }
}

/// Encode text as standard (padded) base64 of its UTF-8 bytes.
fn encode_text(input: &str) -> String {
    STANDARD.encode(input.as_bytes())
}

/// Decode base64 text.
///
/// Accepts the standard and URL-safe alphabets, padded or not, and ignores
/// whitespace inside the input — base64 pasted from a file or a mail header
/// arrives wrapped across lines.
///
/// Returns `None` when the input is not base64 or does not decode to UTF-8:
/// the only thing this plugin can hand back is text on the clipboard, so a
/// binary payload has nothing to show.
fn decode_text(input: &str) -> Option<String> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }

    let bytes = STANDARD
        .decode(&compact)
        .or_else(|_| STANDARD_NO_PAD.decode(&compact))
        .or_else(|_| URL_SAFE.decode(&compact))
        .or_else(|_| URL_SAFE_NO_PAD.decode(&compact))
        .ok()?;

    String::from_utf8(bytes).ok()
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

pub struct Base64Plugin {
    manifest: PluginManifest,
    app: AppHandle,
}

impl Base64Plugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: PLUGIN_ID.into(),
                name: "Base64".into(),
                description: "Encode and decode base64 text".into(),
                icon: "base64".into(),
                keyword: Some("b64".into()),
                // Every string is encodable, so a general fan-out would add a
                // base64 row to every single query — require the keyword.
                keyword_only: true,
            },
            app,
        }
    }

    fn to_result(direction: Direction, value: String, source: &str, score: f64) -> SearchResult {
        SearchResult {
            id: format!("{}:{}:{}", PLUGIN_ID, direction.id(), value),
            plugin_id: PLUGIN_ID.into(),
            title: preview(&value),
            subtitle: Some(crate::i18n::tf(
                "plugins",
                "base64_subtitle",
                &[
                    ("direction", direction.label()),
                    ("source", &preview(source)),
                    ("chars", &value.chars().count().to_string()),
                ],
            )),
            icon: ResultIcon::Named("code".into()),
            score,
            actions: vec![Action {
                id: "copy".into(),
                title: crate::i18n::t("plugins", "copy_result").into(),
                shortcut: Some("Enter".into()),
            }],
            match_indices: vec![],
        }
    }
}

#[async_trait]
impl ConduitPlugin for Base64Plugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let input = query.trim();
        if input.is_empty() {
            return vec![];
        }

        let decoded = decode_text(input);
        let mut results = Vec::new();

        // Decode leads when the input is decodable: someone who pastes base64
        // wants the plain text, and plain words rarely decode to valid UTF-8.
        if let Some(decoded) = decoded {
            results.push(Self::to_result(Direction::Decode, decoded, input, 1.0));
            results.push(Self::to_result(
                Direction::Encode,
                encode_text(input),
                input,
                0.9,
            ));
        } else {
            results.push(Self::to_result(
                Direction::Encode,
                encode_text(input),
                input,
                1.0,
            ));
        }

        results
    }

    async fn execute(&self, result_id: &str, _action_id: &str) -> anyhow::Result<()> {
        // result_id format: "conduit.base64:<direction>:<value>". The value is
        // the whole tail, so it may itself contain ':' or newlines.
        let value = result_id
            .strip_prefix(&format!("{}:", PLUGIN_ID))
            .and_then(|rest| rest.split_once(':'))
            .map(|(_, value)| value)
            .ok_or_else(|| anyhow::anyhow!("malformed base64 result id: {}", result_id))?;

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
    fn encodes_ascii_and_utf8() {
        assert_eq!(encode_text("hello"), "aGVsbG8=");
        assert_eq!(encode_text("こんにちは"), "44GT44KT44Gr44Gh44Gv");
    }

    #[test]
    fn decodes_what_it_encodes() {
        for text in ["hello", "こんにちは", "a:b:c", "line1\nline2", "🔤 emoji"] {
            assert_eq!(decode_text(&encode_text(text)).as_deref(), Some(text));
        }
    }

    #[test]
    fn decodes_unpadded_and_url_safe_input() {
        // The last pair only differs in the alphabet: - _ vs + /
        assert_eq!(decode_text("aGVsbG8").as_deref(), Some("hello"));
        assert_eq!(decode_text("8J-UpCBh").as_deref(), Some("🔤 a"));
        assert_eq!(decode_text("8J+UpCBh").as_deref(), Some("🔤 a"));
    }

    #[test]
    fn ignores_whitespace_in_wrapped_input() {
        assert_eq!(decode_text("aGVs\nbG8=").as_deref(), Some("hello"));
        assert_eq!(decode_text("  aGVs bG8=  ").as_deref(), Some("hello"));
    }

    #[test]
    fn rejects_non_base64_and_non_utf8_payloads() {
        assert_eq!(decode_text(""), None);
        assert_eq!(decode_text("   "), None);
        // Not the base64 alphabet
        assert_eq!(decode_text("hello world!"), None);
        // Valid base64, but the bytes are not text — nothing to copy
        assert_eq!(decode_text("abcd"), None);
        assert_eq!(decode_text("//4="), None);
    }

    #[test]
    fn preview_truncates_and_flattens() {
        assert_eq!(preview("line1\nline2"), "line1 line2");
        let long = "a".repeat(PREVIEW_LEN + 10);
        let shown = preview(&long);
        assert!(shown.ends_with('…'));
        assert_eq!(shown.chars().count(), PREVIEW_LEN + 1);
    }
}
