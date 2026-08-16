//! Everything that has been captured, most recent first.
//!
//! Screenshots and recordings are kept **inside the app** rather than
//! dropped into Pictures: what the launcher took is the launcher's to
//! show, to page through and to prune, and a folder that grows without
//! anyone deciding it should is how a capture tool becomes a disk
//! problem. Files live next to the config, the index next to them, and
//! the list is capped — the oldest is deleted when a new one arrives.
//!
//! Shared app state for the same reason the color history is: the
//! launcher's rows and the gallery window are two views of one list, and
//! the protocol handler that serves the gallery can reach app state but
//! not a plugin instance.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::capture::Shot;

/// Enough to cover a session of grabbing screens, capped because these are
/// megabytes each and nobody prunes a folder they never look at.
pub const MAX_HISTORY: usize = 24;

/// Longest edge of the preview kept alongside each capture. Big enough to
/// recognise a window in the gallery, small enough that the whole history
/// can be handed to a page as data URLs.
pub const THUMB_EDGE: u32 = 360;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Video,
}

impl Kind {
    pub const fn is_video(self) -> bool {
        matches!(self, Kind::Video)
    }
}

/// One capture. `file` and `thumb` are names inside the captures
/// directory, never paths: the index is rewritten on every change, and a
/// stored absolute path would be wrong the moment a profile moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub id: String,
    pub kind: Kind,
    pub file: String,
    pub thumb: String,
    pub width: u32,
    pub height: u32,
    pub created_ms: u64,
    pub bytes: u64,
    /// Length of a recording. Zero for a still.
    #[serde(default)]
    pub seconds: f64,
}

/// A capture plus its preview, which is how both the gallery window and
/// the command that refreshes it see the history.
#[derive(Debug, Clone, Serialize)]
pub struct GalleryEntry {
    #[serde(flatten)]
    pub capture: Capture,
    pub thumb_url: String,
}

#[derive(Clone)]
pub struct CaptureStore {
    entries: Arc<RwLock<VecDeque<Capture>>>,
    dir: PathBuf,
}

impl CaptureStore {
    pub fn load(config_dir: &Path) -> Self {
        let dir = config_dir.join("captures");
        let _ = std::fs::create_dir_all(&dir);

        let index: Vec<Capture> = std::fs::read_to_string(dir.join("index.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();

        // A file someone deleted by hand is gone; keeping its row would
        // offer a thumbnail that opens nothing.
        let entries: VecDeque<Capture> = index
            .into_iter()
            .filter(|entry| dir.join(&entry.file).is_file())
            .collect();

        let store = Self {
            entries: Arc::new(RwLock::new(entries)),
            dir,
        };
        store.write_index();
        store
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where a capture's file (or its thumbnail) lives.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Synchronous, like the color history and for the same reason: the
    /// protocol handler reads it while building a response.
    pub fn snapshot(&self) -> Vec<Capture> {
        self.entries
            .read()
            .map(|entries| entries.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get(&self, id: &str) -> Option<Capture> {
        self.entries
            .read()
            .ok()?
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// A still: the PNG and its preview, written and remembered.
    pub fn add_image(&self, shot: &Shot, created_ms: u64) -> Result<Capture, String> {
        let png = shot.to_png()?;
        let thumb = shot.thumbnail(THUMB_EDGE).to_png()?;
        let id = self.unique_id(created_ms, "png");

        let file = format!("shot-{}.png", id);
        let thumb_name = format!("thumb-{}.png", id);
        std::fs::write(self.path(&file), &png).map_err(|e| write_error(&e))?;
        std::fs::write(self.path(&thumb_name), &thumb).map_err(|e| write_error(&e))?;

        Ok(self.remember(Capture {
            id,
            kind: Kind::Image,
            file,
            thumb: thumb_name,
            width: shot.width,
            height: shot.height,
            created_ms,
            bytes: png.len() as u64,
            seconds: 0.0,
        }))
    }

    /// Where a recording should write itself. A recording streams into its
    /// file over however long it runs, so it takes the name up front and
    /// only becomes an entry once it has stopped.
    pub fn reserve_video(&self, created_ms: u64, extension: &str) -> (String, PathBuf) {
        let id = self.unique_id(created_ms, extension);
        let file = format!("rec-{}.{}", id, extension);
        let path = self.path(&file);
        (id, path)
    }

    /// Remember a recording that has finished. `poster` is the still taken
    /// when it started — a video has no thumbnail of its own that we can
    /// read back without decoding it.
    pub fn add_video(
        &self,
        id: String,
        file: String,
        poster: &Shot,
        seconds: f64,
        created_ms: u64,
    ) -> Result<Capture, String> {
        let path = self.path(&file);
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if bytes == 0 {
            let _ = std::fs::remove_file(&path);
            return Err(crate::i18n::t("errors", "record_empty").to_string());
        }

        let thumb_name = format!("thumb-{}.png", id);
        let thumb = poster.thumbnail(THUMB_EDGE).to_png()?;
        std::fs::write(self.path(&thumb_name), &thumb).map_err(|e| write_error(&e))?;

        Ok(self.remember(Capture {
            id,
            kind: Kind::Video,
            file,
            thumb: thumb_name,
            width: poster.width,
            height: poster.height,
            created_ms,
            bytes,
            seconds,
        }))
    }

    /// Forget one capture, taking its files with it. The history is the
    /// only place these exist, so deleting a row has to delete the bytes.
    pub fn remove(&self, id: &str) -> bool {
        let removed = {
            let Ok(mut entries) = self.entries.write() else {
                return false;
            };
            let Some(position) = entries.iter().position(|entry| entry.id == id) else {
                return false;
            };
            entries.remove(position)
        };
        let Some(entry) = removed else {
            return false;
        };
        self.delete_files(&entry);
        self.write_index();
        true
    }

    pub fn clear(&self) {
        let entries: Vec<Capture> = {
            let Ok(mut entries) = self.entries.write() else {
                return;
            };
            entries.drain(..).collect()
        };
        for entry in &entries {
            self.delete_files(entry);
        }
        self.write_index();
    }

    /// The history with each preview inlined as a data URL.
    ///
    /// Neither the gallery window nor a launcher row can read a file —
    /// the window has no filesystem access at all, which is the point of
    /// a tool window — so the preview has to travel with the list.
    pub fn gallery(&self) -> Vec<GalleryEntry> {
        self.snapshot()
            .into_iter()
            .map(|capture| GalleryEntry {
                thumb_url: std::fs::read(self.path(&capture.thumb))
                    .map(|bytes| crate::capture::png_data_url(&bytes))
                    .unwrap_or_default(),
                capture,
            })
            .collect()
    }

    /// The same list as JSON, for injection into the gallery window.
    pub fn as_gallery_json(&self) -> String {
        serde_json::to_string(&self.gallery()).unwrap_or_else(|_| "[]".into())
    }

    fn remember(&self, capture: Capture) -> Capture {
        let evicted = {
            let Ok(mut entries) = self.entries.write() else {
                return capture;
            };
            entries.push_front(capture.clone());
            let mut evicted = Vec::new();
            while entries.len() > MAX_HISTORY {
                if let Some(old) = entries.pop_back() {
                    evicted.push(old);
                }
            }
            evicted
        };
        for entry in &evicted {
            self.delete_files(entry);
        }
        self.write_index();
        capture
    }

    fn delete_files(&self, entry: &Capture) {
        let _ = std::fs::remove_file(self.path(&entry.file));
        let _ = std::fs::remove_file(self.path(&entry.thumb));
    }

    fn write_index(&self) {
        let snapshot = self.snapshot();
        if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
            let _ = std::fs::write(self.dir.join("index.json"), json);
        }
    }

    /// A timestamp id, with a counter appended if a capture in the same
    /// millisecond already took it. Two screenshots can share a
    /// millisecond; two files cannot share a name.
    fn unique_id(&self, created_ms: u64, extension: &str) -> String {
        let base = stamp(created_ms);
        let taken = |id: &str| {
            self.path(&format!("shot-{}.png", id)).exists()
                || self.path(&format!("rec-{}.{}", id, extension)).exists()
        };
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{}-{}", base, n))
            .find(|id| !taken(id))
            .unwrap_or(base)
    }
}

fn write_error(e: &std::io::Error) -> String {
    crate::i18n::tf("errors", "capture_write", &[("error", &e.to_string())])
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// `YYYYMMDD-HHMMSS`, in UTC.
///
/// UTC because the only clock available here without another dependency
/// is the epoch, and a file named in a made-up local time would be a lie
/// on the machine that travels. What a person reads is formatted from
/// `created_ms` where a timezone is available: relative in the launcher,
/// local in the gallery window.
pub fn stamp(ms: u64) -> String {
    let seconds = ms / 1000;
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        year,
        month,
        day,
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's civil
/// calendar algorithm, the same one `chrono` implements — spelled out
/// here rather than adding a dependency for one date a screenshot).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// A byte count as something to read in a subtitle.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> (CaptureStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "conduit-captures-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (CaptureStore::load(&dir), dir)
    }

    fn shot(width: u32, height: u32) -> Shot {
        Shot::new(width, height, vec![120; (width * height * 4) as usize])
    }

    #[test]
    fn a_screenshot_lands_on_disk_with_a_preview_beside_it() {
        let (store, dir) = store("image");
        let capture = store.add_image(&shot(200, 100), 1_755_000_000_000).unwrap();

        assert_eq!(capture.kind, Kind::Image);
        assert_eq!((capture.width, capture.height), (200, 100));
        assert!(store.path(&capture.file).is_file());
        assert!(store.path(&capture.thumb).is_file());
        assert_eq!(store.snapshot().len(), 1);

        // and it is still there for the next launch
        assert_eq!(CaptureStore::load(&dir).snapshot(), store.snapshot());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The cap is what keeps this from filling a disk, so the eviction has
    /// to take the bytes with it, not just the row.
    #[test]
    fn the_oldest_capture_is_deleted_with_its_files() {
        let (store, dir) = store("cap");
        let first = store.add_image(&shot(8, 8), 1_755_000_000_000).unwrap();
        for i in 1..=MAX_HISTORY {
            store
                .add_image(&shot(8, 8), 1_755_000_000_000 + i as u64 * 1000)
                .unwrap();
        }

        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), MAX_HISTORY);
        assert!(!snapshot.iter().any(|entry| entry.id == first.id));
        assert!(!store.path(&first.file).exists(), "the file outlived its row");
        assert!(!store.path(&first.thumb).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_a_capture_deletes_its_files() {
        let (store, dir) = store("remove");
        let capture = store.add_image(&shot(16, 16), 1_755_000_000_000).unwrap();
        assert!(store.remove(&capture.id));
        assert!(!store.path(&capture.file).exists());
        assert!(!store.path(&capture.thumb).exists());
        assert!(store.snapshot().is_empty());
        assert!(!store.remove(&capture.id), "removing twice is not a success");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two captures in the same second are ordinary — a key held down, a
    /// recording started right after a still. They must not overwrite each
    /// other's file.
    #[test]
    fn captures_in_the_same_second_get_different_files() {
        let (store, dir) = store("collide");
        let a = store.add_image(&shot(8, 8), 1_755_000_000_000).unwrap();
        let b = store.add_image(&shot(8, 8), 1_755_000_000_400).unwrap();
        assert_ne!(a.file, b.file);
        assert!(store.path(&a.file).is_file() && store.path(&b.file).is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file deleted outside the app leaves a row pointing at nothing;
    /// the next start has to drop it rather than show a dead thumbnail.
    #[test]
    fn a_capture_whose_file_vanished_is_forgotten_on_load() {
        let (store, dir) = store("vanish");
        let capture = store.add_image(&shot(8, 8), 1_755_000_000_000).unwrap();
        std::fs::remove_file(store.path(&capture.file)).unwrap();

        assert!(CaptureStore::load(&dir).snapshot().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_gallery_gets_every_capture_with_its_preview_inlined() {
        let (store, dir) = store("gallery");
        store.add_image(&shot(20, 10), 1_755_000_000_000).unwrap();

        let json: serde_json::Value = serde_json::from_str(&store.as_gallery_json()).unwrap();
        let rows = json.as_array().expect("an array of captures");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["kind"], "image");
        assert!(rows[0]["thumb_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The stamp is a file name, and a wrong date in it is invisible until
    /// someone sorts by name a month later.
    #[test]
    fn the_stamp_is_the_utc_calendar_date() {
        assert_eq!(stamp(0), "19700101-000000");
        // 2026-08-15T09:30:12Z
        assert_eq!(stamp(1_786_786_212_000), "20260815-093012");
        // a leap day, which is where a hand-rolled calendar goes wrong
        assert_eq!(stamp(1_709_164_800_000), "20240229-000000");
    }

    #[test]
    fn sizes_read_the_way_a_person_would_say_them() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }
}
