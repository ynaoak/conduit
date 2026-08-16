//! The colors that have been picked, most recent first.
//!
//! Shared state rather than a field on the plugin: the launcher's rows and
//! the Color Palette tool window are two views of one list, and a tool
//! window is served from the protocol handler, which can reach app state
//! but not a plugin instance.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};


use crate::color::Rgb;

/// Long enough to cover a session of sampling a design, short enough that
/// the list stays scannable.
pub const MAX_HISTORY: usize = 24;

#[derive(Clone)]
pub struct ColorHistoryState {
    colors: Arc<RwLock<VecDeque<Rgb>>>,
    path: PathBuf,
}

impl ColorHistoryState {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join("colors.json");
        // A history that will not parse is not worth failing over; the next
        // pick rewrites it.
        let colors = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<Rgb>>(&text).ok())
            .unwrap_or_default();

        Self {
            colors: Arc::new(RwLock::new(colors.into())),
            path,
        }
    }

    /// Synchronous on purpose: the tool-window protocol handler reads this
    /// while building a response, and the critical sections are a few
    /// dozen colors long.
    pub fn snapshot(&self) -> Vec<Rgb> {
        self.colors
            .read()
            .map(|colors| colors.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Put `color` at the front, writing the list back to disk.
    pub fn remember(&self, color: Rgb) {
        let snapshot = {
            let Ok(mut colors) = self.colors.write() else {
                return;
            };
            // A color picked again moves to the front rather than appearing
            // twice — this is "what I have been using", not a log.
            colors.retain(|existing| *existing != color);
            colors.push_front(color);
            colors.truncate(MAX_HISTORY);
            colors.iter().copied().collect::<Vec<_>>()
        };
        if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    /// The history as a JSON array of hex strings, for injection into a
    /// tool window. Hex rather than objects because that is what the page
    /// displays, parses and copies.
    pub fn as_hex_json(&self) -> String {
        let hexes: Vec<String> = self.snapshot().iter().map(Rgb::hex).collect();
        serde_json::to_string(&hexes).unwrap_or_else(|_| "[]".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(dir: &Path) -> ColorHistoryState {
        ColorHistoryState::load(dir)
    }

    #[test]
    fn a_repeated_color_moves_to_the_front_instead_of_repeating() {
        let dir = std::env::temp_dir().join(format!("conduit-colors-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let history = state(&dir);

        let red = Rgb::new(255, 0, 0);
        let blue = Rgb::new(0, 0, 255);
        history.remember(red);
        history.remember(blue);
        history.remember(red);

        assert_eq!(history.snapshot(), vec![red, blue]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_history_stops_growing_at_its_limit() {
        let dir = std::env::temp_dir().join(format!("conduit-colors-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let history = state(&dir);

        for i in 0..(MAX_HISTORY + 10) {
            history.remember(Rgb::new(i as u8, 0, 0));
        }
        let snapshot = history.snapshot();
        assert_eq!(snapshot.len(), MAX_HISTORY);
        // Newest first: the last color pushed is the one at the front
        assert_eq!(snapshot[0], Rgb::new((MAX_HISTORY + 9) as u8, 0, 0));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The tool window reads this as JSON and parses each entry back into a
    /// color, so the shape matters as much as the contents.
    #[test]
    fn the_injected_form_is_an_array_of_hex_strings() {
        let dir = std::env::temp_dir().join(format!("conduit-colors-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let history = state(&dir);

        assert_eq!(history.as_hex_json(), "[]");
        history.remember(Rgb::new(0xff, 0x88, 0x00));
        assert_eq!(history.as_hex_json(), r##"["#ff8800"]"##);
        std::fs::remove_dir_all(&dir).ok();
    }
}
