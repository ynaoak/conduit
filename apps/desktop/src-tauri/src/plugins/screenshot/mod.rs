//! Screenshots and recordings from the launcher.
//!
//! The capturing itself is `capture`; what lives here is the launcher side
//! of it — the rows that start one, and the history that makes the last
//! two dozen reachable without digging through a folder. A capture row
//! carries its own preview as its icon, for the same reason a picked color
//! carries its swatch: a list of identical camera icons tells you nothing
//! about which shot you are looking for.

use async_trait::async_trait;
use tauri::{AppHandle, Manager};

use crate::capture::{self, record, Screen};
use crate::capture_history::{human_size, Capture, CaptureStore, Kind};
use crate::commands::capture::Mode;
use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

const PLUGIN_ID: &str = "conduit.screenshot";

/// The rows that start something, rather than showing something taken
const REGION_ID: &str = "conduit.screenshot:region";
const SCREEN_ID: &str = "conduit.screenshot:screen";
/// One monitor by itself: `conduit.screenshot:screen:1`
const SCREEN_PREFIX: &str = "conduit.screenshot:screen:";
const RECORD_ID: &str = "conduit.screenshot:record";
const STOP_ID: &str = "conduit.screenshot:stop";

/// How many past captures a keyword listing shows
const DEFAULT_LISTING: usize = 6;

/// The words that should surface this without a keyword, in either
/// language — someone typing "スクショ" wants the screenshot tool whatever
/// the UI happens to be set to.
const ALIASES: &[&str] = &[
    "screenshot", "screen", "shot", "capture", "snip", "grab", "region", "record", "recording",
    "video", "gif", "スクショ", "スクリーンショット", "キャプチャ", "画面", "録画", "撮影",
];

pub struct ScreenshotPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    captures: CaptureStore,
}

impl ScreenshotPlugin {
    pub fn new(app: AppHandle) -> Self {
        // The gallery window reads the same list, so it lives in app state
        let captures = app.state::<CaptureStore>().inner().clone();

        Self {
            manifest: PluginManifest {
                id: PLUGIN_ID.into(),
                name: "Screenshot".into(),
                description: "Capture a region, a screen, or a recording".into(),
                icon: "screenshot_monitor".into(),
                // Keyword-free, like the color picker: an empty query is
                // the dashboard, which is for what the user has been doing
                // rather than a menu of features. Asking for it by name
                // goes through ALIASES in the general fan-out.
                keyword: None,
                keyword_only: false,
            },
            app,
            captures,
        }
    }

    fn action(id: &str, key: &str) -> Action {
        Action {
            id: id.into(),
            title: crate::i18n::t("plugins", key).into(),
            shortcut: None,
        }
    }

    fn starter(id: &str, title_key: &str, subtitle: String, icon: &str, score: f64) -> SearchResult {
        SearchResult {
            id: id.into(),
            plugin_id: PLUGIN_ID.into(),
            title: crate::i18n::t("plugins", title_key).to_string(),
            subtitle: Some(subtitle),
            icon: ResultIcon::Named(icon.into()),
            score,
            actions: vec![],
            match_indices: Vec::new(),
        }
    }

    /// A row for something already captured. The preview is the icon; the
    /// subtitle is what a person needs to tell two screenshots apart —
    /// when, how big, and how heavy.
    fn capture_result(&self, capture: &Capture, score: f64) -> SearchResult {
        let when = crate::i18n::relative_time(seconds_since(capture.created_ms));
        let subtitle = if capture.kind.is_video() {
            crate::i18n::tf(
                "plugins",
                "capture_subtitle_video",
                &[
                    ("when", &when),
                    ("seconds", &format!("{:.0}", capture.seconds)),
                    ("size", &human_size(capture.bytes)),
                ],
            )
        } else {
            crate::i18n::tf(
                "plugins",
                "capture_subtitle_image",
                &[
                    ("when", &when),
                    ("width", &capture.width.to_string()),
                    ("height", &capture.height.to_string()),
                    ("size", &human_size(capture.bytes)),
                ],
            )
        };

        SearchResult {
            id: format!("{}:{}", PLUGIN_ID, capture.id),
            plugin_id: PLUGIN_ID.into(),
            title: title_for(capture),
            subtitle: Some(subtitle),
            icon: self.preview(capture),
            score,
            actions: vec![
                Self::action(
                    "copy",
                    if capture.kind.is_video() {
                        "capture_copy_path"
                    } else {
                        "capture_copy_image"
                    },
                ),
                Self::action("open", "open"),
                Self::action("reveal", "open_folder"),
                Self::action("delete", "delete_entry"),
            ],
            match_indices: Vec::new(),
        }
    }

    fn preview(&self, capture: &Capture) -> ResultIcon {
        std::fs::read(self.captures.path(&capture.thumb))
            .ok()
            .map(|bytes| {
                use base64::engine::general_purpose::STANDARD;
                use base64::Engine;
                ResultIcon::Base64(STANDARD.encode(bytes))
            })
            .unwrap_or_else(|| ResultIcon::Named(icon_for(capture).into()))
    }

    /// The rows that do something, in the order they are worth offering.
    /// While a recording is running, the only thing anyone wants from
    /// this plugin is to stop it.
    fn starters(&self) -> Vec<SearchResult> {
        if record::is_running() {
            return vec![Self::starter(
                STOP_ID,
                "capture_stop_title",
                crate::i18n::t("plugins", "capture_stop_subtitle").to_string(),
                "stop_circle",
                1.0,
            )];
        }

        let screens = capture::screens(&self.app);
        let several = screens.len() > 1;

        let mut rows = vec![
            Self::starter(
                REGION_ID,
                "capture_region_title",
                crate::i18n::t(
                    "plugins",
                    // With more than one display, the thing worth saying
                    // is that the other ones are selectable too — that is
                    // the whole question a second monitor raises.
                    if several {
                        "capture_region_subtitle_multi"
                    } else {
                        "capture_region_subtitle"
                    },
                )
                .to_string(),
                "crop_free",
                0.97,
            ),
            Self::starter(
                SCREEN_ID,
                if several { "capture_screens_title" } else { "capture_screen_title" },
                if several {
                    crate::i18n::tf(
                        "plugins",
                        "capture_screens_subtitle",
                        &[("n", &screens.len().to_string())],
                    )
                } else {
                    crate::i18n::t("plugins", "capture_screen_subtitle").to_string()
                },
                "screenshot_monitor",
                0.96,
            ),
        ];

        // Each display on its own, so "the other screen" is one row away
        // rather than a crop out of a composite.
        if several {
            for screen in &screens {
                rows.push(Self::screen_result(screen, 0.955 - screen.index as f64 * 0.001));
            }
        }

        if record::is_supported() {
            rows.push(Self::starter(
                RECORD_ID,
                "capture_record_title",
                // The cap is part of the promise: a recording that stops
                // on its own is only reasonable if it said it would.
                crate::i18n::tf(
                    "plugins",
                    "capture_record_subtitle",
                    &[("max", &record::MAX_SECONDS.to_string())],
                ),
                "videocam",
                0.95,
            ));
        }
        rows
    }

    /// A row for one display, numbered the way `screens()` orders them —
    /// left to right, which is how they are arranged on the desk.
    fn screen_result(screen: &Screen, score: f64) -> SearchResult {
        SearchResult {
            id: format!("{}{}", SCREEN_PREFIX, screen.index),
            plugin_id: PLUGIN_ID.into(),
            title: crate::i18n::tf(
                "plugins",
                "capture_one_screen_title",
                &[("n", &(screen.index + 1).to_string())],
            ),
            subtitle: Some(crate::i18n::tf(
                "plugins",
                "capture_one_screen_subtitle",
                &[
                    ("width", &screen.bounds.width.to_string()),
                    ("height", &screen.bounds.height.to_string()),
                ],
            )),
            icon: ResultIcon::Named("monitor".into()),
            score,
            actions: vec![],
            match_indices: Vec::new(),
        }
    }
}

fn title_for(capture: &Capture) -> String {
    crate::i18n::t(
        "plugins",
        match capture.kind {
            Kind::Image => "capture_title_image",
            Kind::Video => "capture_title_video",
        },
    )
    .to_string()
}

fn icon_for(capture: &Capture) -> &'static str {
    match capture.kind {
        Kind::Image => "image",
        Kind::Video => "movie",
    }
}

fn seconds_since(created_ms: u64) -> u64 {
    crate::capture_history::now_ms().saturating_sub(created_ms) / 1000
}

#[async_trait]
impl ConduitPlugin for ScreenshotPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        if !capture::is_supported() {
            return vec![];
        }
        let query = query.trim().to_lowercase();

        // The empty query is the dashboard, which is for what the user has
        // been doing rather than a menu of what the launcher can do.
        if query.is_empty() {
            return vec![];
        }

        if ALIASES.iter().any(|alias| alias.starts_with(&query)) {
            let mut results = self.starters();
            results.extend(
                self.captures
                    .snapshot()
                    .iter()
                    .take(DEFAULT_LISTING)
                    .enumerate()
                    .map(|(i, capture)| self.capture_result(capture, 0.9 - i as f64 * 0.02)),
            );
            return results;
        }

        vec![]
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let take = |mode, screen| {
            let app = self.app.clone();
            let captures = self.captures.clone();
            async move {
                crate::commands::capture::take(&app, &captures, mode, screen, true)
                    .await
                    .map_err(|message| anyhow::anyhow!(message))
                    .map(|_| ())
            }
        };

        if let Some(index) = result_id.strip_prefix(SCREEN_PREFIX) {
            let index = index
                .parse()
                .map_err(|_| anyhow::anyhow!("not a screen result id: {}", result_id))?;
            return take(Mode::Screen, Some(index)).await;
        }

        match result_id {
            REGION_ID => return take(Mode::Region, None).await,
            SCREEN_ID => return take(Mode::Screen, None).await,
            RECORD_ID => return take(Mode::Record, None).await,
            STOP_ID => {
                record::stop().await.map_err(|message| anyhow::anyhow!(message))?;
                return Ok(());
            }
            _ => {}
        }

        let id = result_id
            .strip_prefix(&format!("{}:", PLUGIN_ID))
            .ok_or_else(|| anyhow::anyhow!("not a capture result id: {}", result_id))?;
        let capture = self
            .captures
            .get(id)
            .ok_or_else(|| anyhow::anyhow!(crate::i18n::t("errors", "capture_missing")))?;

        let path = self.captures.path(&capture.file);
        match action_id {
            "open" => {
                use tauri_plugin_opener::OpenerExt;
                self.app.opener().open_path(path.to_string_lossy(), None::<&str>)?;
            }
            "reveal" => {
                use tauri_plugin_opener::OpenerExt;
                self.app.opener().reveal_item_in_dir(&path)?;
            }
            "delete" => {
                self.captures.remove(id);
            }
            // An unknown action is the default one, which is what pressing
            // Enter on the row does: put it on the clipboard.
            _ => crate::commands::capture::copy(&self.app, &self.captures, &capture)
                .map_err(|message| anyhow::anyhow!(message))?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows that start a capture go through the same `execute` as the
    /// rows that show one, and a collision would have the launcher looking
    /// for a capture called "region".
    #[test]
    fn a_starter_row_is_not_a_capture_id() {
        let one_screen = format!("{}{}", SCREEN_PREFIX, 1);
        for id in [REGION_ID, SCREEN_ID, RECORD_ID, STOP_ID, &one_screen] {
            let suffix = id.strip_prefix(&format!("{}:", PLUGIN_ID)).expect("prefixed");
            assert!(
                !suffix.chars().next().unwrap().is_ascii_digit(),
                "{} could be mistaken for a capture id, which is a timestamp",
                id
            );
        }
    }

    /// Every alias has to be reachable by typing its first letters, which
    /// is what the search does — an alias with leading whitespace or in
    /// mixed case would never match.
    #[test]
    fn aliases_are_lowercase_and_trimmed() {
        for alias in ALIASES {
            assert_eq!(*alias, alias.trim(), "{:?} has stray whitespace", alias);
            assert_eq!(*alias, alias.to_lowercase(), "{:?} is not lowercase", alias);
        }
    }
}
