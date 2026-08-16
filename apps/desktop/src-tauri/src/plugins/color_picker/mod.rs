//! Pick a color from anywhere on screen, and keep the ones already picked.
//!
//! The picking itself is `screen_color`; what lives here is the launcher
//! side of it — the row that starts a session, the history that makes the
//! last few colors reachable without picking again, and the formats each
//! one can be copied in.

use async_trait::async_trait;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::color::{swatch_png, Format, Rgb};
use crate::color_history::ColorHistoryState;
use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;
use crate::screen_color;

const PLUGIN_ID: &str = "conduit.color-picker";
/// Result id of the row that starts a pick
const PICK_ID: &str = "conduit.color-picker:pick";
/// How many to show for a bare keyword / the dashboard
const DEFAULT_LISTING: usize = 8;

/// The words that should surface the picker without its keyword. Kept here
/// rather than in the locale files because a user typing "spuit" or "色"
/// wants the picker whichever language the UI happens to be in.
const ALIASES: &[&str] = &[
    "color", "colour", "pick", "picker", "eyedropper", "dropper", "hex", "rgb", "hsl",
    "カラー", "いろ", "色", "スポイト", "ピッカー",
];

pub struct ColorPickerPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    history: ColorHistoryState,
}

impl ColorPickerPlugin {
    pub fn new(app: AppHandle) -> Self {
        // The tool window reads the same list, so it lives in app state
        let history = app.state::<ColorHistoryState>().inner().clone();

        Self {
            manifest: PluginManifest {
                id: PLUGIN_ID.into(),
                name: "Color Picker".into(),
                description: "Pick a color from anywhere on screen".into(),
                icon: "colorize".into(),
                // No keyword: an empty query means the dashboard, and the
                // dashboard is for what the user has been doing rather than
                // a menu of features. Asking for the picker by name goes
                // through ALIASES in the general fan-out instead.
                keyword: None,
                keyword_only: false,
            },
            app,
            history,
        }
    }

    /// Result id for a remembered color. The hex is the whole identity, so
    /// a pin made today still resolves after a restart.
    fn history_id(color: Rgb) -> String {
        format!("{}:{}", PLUGIN_ID, color.hex())
    }

    fn copy_actions() -> Vec<Action> {
        vec![
            Action {
                id: Format::Hex.id().into(),
                title: crate::i18n::t("plugins", "color_copy_hex").into(),
                shortcut: None,
            },
            Action {
                id: Format::Rgb.id().into(),
                title: crate::i18n::t("plugins", "color_copy_rgb").into(),
                shortcut: None,
            },
            Action {
                id: Format::Hsl.id().into(),
                title: crate::i18n::t("plugins", "color_copy_hsl").into(),
                shortcut: None,
            },
        ]
    }

    fn color_to_result(color: Rgb, score: f64) -> SearchResult {
        SearchResult {
            id: Self::history_id(color),
            plugin_id: PLUGIN_ID.into(),
            title: color.hex(),
            subtitle: Some(format!("{} ・ {}", color.rgb_css(), color.hsl_css())),
            // The swatch is the color itself; a named icon here would make
            // every remembered color look the same in the list.
            icon: swatch_png(color)
                .map(ResultIcon::Base64)
                .unwrap_or_else(|| ResultIcon::Named("colorize".into())),
            score,
            actions: Self::copy_actions(),
            match_indices: Vec::new(),
        }
    }

    fn pick_result(score: f64) -> SearchResult {
        SearchResult {
            id: PICK_ID.into(),
            plugin_id: PLUGIN_ID.into(),
            title: crate::i18n::t("plugins", "color_pick_title").to_string(),
            // Escape and right-click are our session's doing; where the
            // system runs the sampler, promising them would be inventing
            // behaviour we do not control.
            subtitle: Some(
                crate::i18n::t(
                    "plugins",
                    if screen_color::platform_draws_the_loupe() {
                        "color_pick_subtitle_sampler"
                    } else {
                        "color_pick_subtitle"
                    },
                )
                .to_string(),
            ),
            icon: ResultIcon::Named("colorize".into()),
            score,
            actions: vec![],
            match_indices: Vec::new(),
        }
    }

    /// Run one pick session and record the result.
    async fn run_pick(&self) -> anyhow::Result<()> {
        // The launcher is opaque and sits over whatever is being sampled,
        // so it has to be out of the way before the session starts.
        crate::commands::color::run_pick(&self.app, &self.history, true)
            .await
            .map_err(|message| anyhow::anyhow!(message))?;
        Ok(())
    }
}

#[async_trait]
impl ConduitPlugin for ColorPickerPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        if !screen_color::is_supported() {
            return vec![];
        }
        let query = query.trim();
        let history = self.history.snapshot();

        // The empty query is the dashboard — frequent apps, open windows,
        // recent clips. A row that only offers to start a tool does not
        // belong at the top of it.
        if query.is_empty() {
            return vec![];
        }

        // A hex someone typed or pasted is a color in its own right: it can
        // be converted without ever having been on screen.
        if let Some(color) = Rgb::parse_hex(query) {
            let mut result = Self::color_to_result(color, 1.0);
            result.subtitle = Some(crate::i18n::t("plugins", "color_from_text").to_string());
            return vec![result];
        }

        let lowered = query.to_lowercase();
        let mut results = Vec::new();
        if ALIASES.iter().any(|alias| alias.starts_with(&lowered)) {
            // Asked for by name: the picker, and everything already picked
            results.push(Self::pick_result(0.95));
            results.extend(
                history
                    .iter()
                    .take(DEFAULT_LISTING)
                    .enumerate()
                    .map(|(i, color)| Self::color_to_result(*color, 0.9 - i as f64 * 0.02)),
            );
            return results;
        }

        // Typing part of a hex narrows the history — "ff8" finds #ff8800
        results.extend(
            history
                .iter()
                .filter(|color| color.hex().contains(&lowered))
                .take(DEFAULT_LISTING)
                .map(|color| Self::color_to_result(*color, 0.8)),
        );
        results
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        if result_id == PICK_ID {
            return self.run_pick().await;
        }

        let hex = result_id.strip_prefix(&format!("{}:", PLUGIN_ID)).unwrap_or(result_id);
        let color = Rgb::parse_hex(hex)
            .ok_or_else(|| anyhow::anyhow!("not a color result id: {}", result_id))?;

        // An unknown action id is the default one: copy the hex, which is
        // what pressing Enter on the row should do.
        let format = Format::from_id(action_id).unwrap_or(Format::Hex);
        self.history.remember(color);
        self.app
            .clipboard()
            .write_text(format.render(color))
            .map_err(|e| anyhow::anyhow!("failed to copy to clipboard: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action the row offers has to map back to a format, or pressing
    /// it silently copies the hex instead of what the label promised.
    #[test]
    fn every_offered_action_names_a_real_format() {
        for action in ColorPickerPlugin::copy_actions() {
            assert!(
                Format::from_id(&action.id).is_some(),
                "action {} has no format",
                action.id
            );
        }
    }

    /// The id is the only thing that survives a pin or a restart, so it has
    /// to round-trip back to the same color.
    #[test]
    fn a_history_id_round_trips_through_the_hex() {
        let color = Rgb::new(0xff, 0x88, 0x00);
        let id = ColorPickerPlugin::history_id(color);
        let hex = id.strip_prefix(&format!("{}:", PLUGIN_ID)).expect("prefixed");
        assert_eq!(Rgb::parse_hex(hex), Some(color));
    }

    /// The pick row must never be mistaken for a color row: they go through
    /// the same `execute`, and a collision would try to parse "pick" as hex.
    #[test]
    fn the_pick_row_is_not_a_color_id() {
        let hex = PICK_ID.strip_prefix(&format!("{}:", PLUGIN_ID)).expect("prefixed");
        assert_eq!(Rgb::parse_hex(hex), None);
    }
}
