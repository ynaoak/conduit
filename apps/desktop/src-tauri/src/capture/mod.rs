//! Reading rectangles of the screen: the still a screenshot is, and the
//! frames a recording is made of.
//!
//! Everything that is arithmetic — where a drag ends up in pixels, how a
//! frame is cropped, how a thumbnail is shrunk — lives here so it can be
//! tested on whatever machine the tests run on. The two platform files
//! answer one question each: give me these pixels.
//!
//! Multiple monitors are the normal case, so they are the case this is
//! built around: every display is frozen, every display gets a selection
//! surface of its own, and a full-screen shot is all of them composited
//! into one image at their real positions.
//!
//! One surface per monitor rather than one window stretched across the
//! desktop, because a window that straddles displays is a different
//! thing on every platform (macOS gives each display its own space).
//! The cost is that a single rectangle cannot span two monitors — which
//! is a rectangle nobody drags on purpose, while "let me grab something
//! on the other screen" is what people actually want.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod macos;
pub mod record;
pub mod region;
#[cfg(windows)]
mod windows;

/// A rectangle in physical pixels. Used both for screen coordinates and
/// for offsets inside a captured frame; which one it is is always clear
/// from the name of what holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// The monitor a capture is happening on: where it is, and how many
/// physical pixels one logical point buys there. The scale matters
/// because macOS's capture tool is asked for points while it answers in
/// pixels.
///
/// `index` is this monitor's place in `screens()`, which is what the
/// selection surfaces, the launcher rows and the gallery all name a
/// display by. Ordered by position rather than by whatever the OS
/// enumerates first, so "画面 1" is the leftmost one both times it is
/// asked about.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Screen {
    pub index: usize,
    pub bounds: Rect,
    pub scale: f64,
}

/// A drag on the selection surface, as fractions of the frozen frame.
///
/// Fractions rather than pixels because the surface is a webview: it works
/// in CSS pixels, the frame is in physical ones, and the ratio between
/// them is exactly the kind of thing that is right on one machine and off
/// by a factor of 1.5 on the next. A fraction of the image means the same
/// on both sides.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Fractions {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Smallest selection worth acting on, in physical pixels. Below this a
/// drag is a click that slipped, and answering it with an 3x2 screenshot
/// helps nobody.
pub const MIN_SELECTION: u32 = 8;

impl Fractions {
    /// Map onto a rectangle of `width` x `height` starting at (`x`, `y`).
    /// `None` when the drag was too small to have meant anything.
    fn to_area(&self, x: i32, y: i32, width: u32, height: u32) -> Option<Rect> {
        if width == 0 || height == 0 {
            return None;
        }
        // A drag up and to the left arrives with negative extents; it is
        // the same rectangle.
        let (x0, x1) = min_max(self.x, self.x + self.width);
        let (y0, y1) = min_max(self.y, self.y + self.height);

        let left = scale(x0, width);
        let right = scale(x1, width);
        let top = scale(y0, height);
        let bottom = scale(y1, height);

        let w = right.saturating_sub(left);
        let h = bottom.saturating_sub(top);
        if w < MIN_SELECTION || h < MIN_SELECTION {
            return None;
        }
        Some(Rect::new(x + left as i32, y + top as i32, w, h))
    }

    /// Offsets into a frozen frame, for cropping it.
    pub fn to_offsets(&self, width: u32, height: u32) -> Option<Rect> {
        self.to_area(0, 0, width, height)
    }

    /// The same selection in screen coordinates, for capturing it again —
    /// which is what a recording does, frame after frame.
    pub fn to_screen(&self, screen: Screen) -> Option<Rect> {
        self.to_area(
            screen.bounds.x,
            screen.bounds.y,
            screen.bounds.width,
            screen.bounds.height,
        )
    }
}

fn min_max(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// A fraction onto a pixel offset, clamped to the frame. NaN (a drag the
/// page never finished) clamps to 0 rather than panicking on the cast.
fn scale(fraction: f64, extent: u32) -> u32 {
    if !fraction.is_finite() {
        return 0;
    }
    (fraction.clamp(0.0, 1.0) * extent as f64).round() as u32
}

/// Pixels read off the screen, 8-bit RGBA, row-major.
#[derive(Clone, PartialEq, Eq)]
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Written out rather than derived: a derived one would dump every pixel
/// into a failing assertion, and what tells two frames apart is their
/// size, not the first two thousand bytes of one of them.
impl std::fmt::Debug for Shot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shot({}x{}, {} bytes)", self.width, self.height, self.rgba.len())
    }
}

impl Shot {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self { width, height, rgba }
    }

    /// An opaque black frame to paste monitors into.
    pub fn blank(width: u32, height: u32) -> Self {
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        Self { width, height, rgba }
    }

    /// Draw `other` at (`x`, `y`), clipped to this frame.
    pub fn paste(&mut self, other: &Shot, x: i32, y: i32) {
        for row in 0..other.height {
            let target_y = y + row as i32;
            if target_y < 0 || target_y as u32 >= self.height {
                continue;
            }
            // The overlap of one source row with this frame, in columns
            let from = (-x).max(0).min(other.width as i32) as u32;
            let to = ((self.width as i32 - x).max(0) as u32).min(other.width);
            if from >= to {
                continue;
            }
            let source = (((row * other.width) + from) * 4) as usize;
            let length = ((to - from) * 4) as usize;
            let target = ((target_y as u32 * self.width + (x + from as i32) as u32) * 4) as usize;
            self.rgba[target..target + length]
                .copy_from_slice(&other.rgba[source..source + length]);
        }
    }

    /// The part of this frame inside `area`, whose coordinates are offsets
    /// into it. `None` if the area falls outside the frame entirely.
    pub fn crop(&self, area: Rect) -> Option<Shot> {
        let left = area.x.max(0) as u32;
        let top = area.y.max(0) as u32;
        let right = (left + area.width).min(self.width);
        let bottom = (top + area.height).min(self.height);
        if left >= right || top >= bottom {
            return None;
        }

        let (width, height) = (right - left, bottom - top);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in top..bottom {
            let start = ((row * self.width + left) * 4) as usize;
            let end = start + (width * 4) as usize;
            rgba.extend_from_slice(&self.rgba[start..end]);
        }
        Some(Shot::new(width, height, rgba))
    }

    /// A copy no longer than `max_edge` on its longest side, averaging the
    /// pixels each destination pixel covers.
    ///
    /// Averaged rather than sampled because a thumbnail of a screenshot is
    /// mostly text: dropping pixels turns a paragraph into noise, while an
    /// average keeps it looking like a paragraph.
    pub fn thumbnail(&self, max_edge: u32) -> Shot {
        let longest = self.width.max(self.height);
        if longest == 0 || longest <= max_edge || self.rgba.is_empty() {
            return self.clone();
        }
        let ratio = max_edge as f64 / longest as f64;
        let width = ((self.width as f64 * ratio).round() as u32).max(1);
        let height = ((self.height as f64 * ratio).round() as u32).max(1);

        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let y0 = row * self.height / height;
            let y1 = (((row + 1) * self.height + height - 1) / height).max(y0 + 1).min(self.height);
            for column in 0..width {
                let x0 = column * self.width / width;
                let x1 = (((column + 1) * self.width + width - 1) / width)
                    .max(x0 + 1)
                    .min(self.width);

                let mut sums = [0u64; 4];
                let mut count = 0u64;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = ((y * self.width + x) * 4) as usize;
                        for (channel, sum) in sums.iter_mut().enumerate() {
                            *sum += self.rgba[i + channel] as u64;
                        }
                        count += 1;
                    }
                }
                let count = count.max(1);
                for sum in sums {
                    rgba.push((sum / count) as u8);
                }
            }
        }
        Shot::new(width, height, rgba)
    }

    /// PNG bytes. What goes on the clipboard is the raw frame; this is
    /// what goes on disk.
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| format!("failed to write PNG header: {}", e))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|e| format!("failed to encode PNG: {}", e))?;
        }
        Ok(png)
    }

    /// A `data:` URL of the PNG, which is how a frame reaches a window
    /// that has no way to read a file (the selection surface, the gallery).
    pub fn to_data_url(&self) -> Result<String, String> {
        Ok(png_data_url(&self.to_png()?))
    }
}

/// PNG bytes as a `data:` URL.
pub fn png_data_url(png: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

/// Decode PNG bytes back into a frame. Only used where a platform hands
/// its capture over as a file (macOS), but the decoder is portable, so it
/// is not behind a cfg.
#[allow(dead_code)]
pub fn decode_png(bytes: &[u8]) -> Result<Shot, String> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("failed to read PNG: {}", e))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("failed to decode PNG: {}", e))?;
    buffer.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => return Err(format!("unsupported PNG color type: {:?}", other)),
    };
    Ok(Shot::new(info.width, info.height, rgba))
}

/// True where capturing is implemented. Checked before a row is offered,
/// so a platform without an implementation never shows an action that can
/// only fail.
pub const fn is_supported() -> bool {
    cfg!(windows) || cfg!(target_os = "macos")
}

/// Every monitor, ordered by position: left to right, then top to bottom.
///
/// The order is the display's identity everywhere else in the app, so it
/// has to come from where the monitors are rather than from the order the
/// OS happens to enumerate them in — that order can change between runs,
/// and a "画面 2" that moves is worse than no numbering at all.
pub fn screens(app: &AppHandle) -> Vec<Screen> {
    let Ok(monitors) = app.available_monitors() else {
        return Vec::new();
    };
    let mut bounds: Vec<(Rect, f64)> = monitors
        .into_iter()
        .filter_map(|monitor| {
            let size = monitor.size();
            if size.width == 0 || size.height == 0 {
                return None;
            }
            let position = monitor.position();
            Some((
                Rect::new(position.x, position.y, size.width, size.height),
                monitor.scale_factor(),
            ))
        })
        .collect();
    bounds.sort_by_key(|(rect, _)| (rect.x, rect.y));

    bounds
        .into_iter()
        .enumerate()
        .map(|(index, (bounds, scale))| Screen { index, bounds, scale })
        .collect()
}

/// Read `area` off the screen. `scale` is the monitor's device pixel
/// ratio, which only macOS needs (its capture tool is addressed in points).
pub fn capture(area: Rect, scale: f64) -> Result<Shot, String> {
    if area.is_empty() {
        return Err("nothing to capture".into());
    }
    #[cfg(windows)]
    {
        let _ = scale;
        return windows::capture(area);
    }
    #[cfg(target_os = "macos")]
    {
        return macos::capture(area, scale);
    }
    #[allow(unreachable_code)]
    {
        let _ = (area, scale);
        Err(crate::i18n::t("errors", "capture_unsupported").to_string())
    }
}

/// One frozen monitor: where it is, and what was on it.
pub type Frame = (Screen, Shot);

/// Freeze every monitor.
///
/// All of them, always, even when only one is going to be used: the
/// selection surfaces go up together and have to show the same instant,
/// and a display captured a second later has moved on (a clock ticked, a
/// video advanced) in a way that is obvious the moment two surfaces are
/// side by side.
pub fn capture_all(app: &AppHandle) -> Result<Vec<Frame>, String> {
    if !is_supported() {
        return Err(crate::i18n::t("errors", "capture_unsupported").to_string());
    }
    let screens = screens(app);
    if screens.is_empty() {
        return Err("no monitor to capture".into());
    }

    let mut frames = Vec::with_capacity(screens.len());
    for screen in screens {
        // One unreadable monitor does not cancel the others: a display
        // that was unplugged between the enumeration and the capture is
        // a race, not a failure the user should have to retry.
        if let Ok(shot) = capture(screen.bounds, screen.scale) {
            frames.push((screen, shot));
        }
    }
    if frames.is_empty() {
        return Err(crate::i18n::t("errors", "capture_failed").to_string());
    }
    Ok(frames)
}

/// One monitor, frozen.
pub fn capture_one(app: &AppHandle, index: usize) -> Result<Frame, String> {
    let screen = screens(app)
        .into_iter()
        .find(|screen| screen.index == index)
        .ok_or_else(|| crate::i18n::t("errors", "capture_no_screen").to_string())?;
    Ok((screen, capture(screen.bounds, screen.scale)?))
}

/// The rectangle every monitor fits inside — the desktop as one surface.
pub fn virtual_bounds(screens: &[Screen]) -> Option<Rect> {
    let first = screens.first()?.bounds;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x + first.width as i32;
    let mut bottom = first.y + first.height as i32;
    for screen in screens.iter().skip(1) {
        left = left.min(screen.bounds.x);
        top = top.min(screen.bounds.y);
        right = right.max(screen.bounds.x + screen.bounds.width as i32);
        bottom = bottom.max(screen.bounds.y + screen.bounds.height as i32);
    }
    Some(Rect::new(left, top, (right - left) as u32, (bottom - top) as u32))
}

/// Every frozen monitor pasted into one image at its real position.
///
/// Monitors are rarely a tidy row: they differ in size, sit at different
/// heights, and leave gaps between them. The gaps stay black, which is
/// what the desktop has there too — anything else would invent pixels
/// that were never on a screen.
pub fn composite(frames: &[Frame]) -> Option<Shot> {
    if frames.len() == 1 {
        return Some(frames[0].1.clone());
    }
    let screens: Vec<Screen> = frames.iter().map(|(screen, _)| *screen).collect();
    let bounds = virtual_bounds(&screens)?;

    let mut canvas = Shot::blank(bounds.width, bounds.height);
    for (screen, shot) in frames {
        canvas.paste(shot, screen.bounds.x - bounds.x, screen.bounds.y - bounds.y);
    }
    Some(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32) -> Shot {
        // Each pixel carries its own coordinates, so a crop can be checked
        // against where it came from rather than merely against a size.
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        Shot::new(width, height, rgba)
    }

    #[test]
    fn a_drag_maps_onto_the_pixels_it_covered() {
        let selection = Fractions { x: 0.25, y: 0.5, width: 0.5, height: 0.25 };
        assert_eq!(
            selection.to_offsets(800, 600),
            Some(Rect::new(200, 300, 400, 150))
        );
    }

    /// Dragging up and to the left is the same rectangle as dragging down
    /// and to the right, and the surface is allowed to report it either way.
    #[test]
    fn a_backwards_drag_is_the_same_rectangle() {
        let forwards = Fractions { x: 0.2, y: 0.2, width: 0.6, height: 0.6 };
        let backwards = Fractions { x: 0.8, y: 0.8, width: -0.6, height: -0.6 };
        assert_eq!(
            backwards.to_offsets(1000, 1000),
            forwards.to_offsets(1000, 1000)
        );
    }

    /// A click that moved three pixels is not a selection. Answering it
    /// with a screenshot would be answering something nobody asked.
    #[test]
    fn a_drag_too_small_to_mean_anything_is_no_selection() {
        let slip = Fractions { x: 0.5, y: 0.5, width: 0.001, height: 0.001 };
        assert_eq!(slip.to_offsets(1000, 1000), None);
    }

    /// The surface reports fractions of what it drew; a drag released off
    /// the edge of the window still has to land inside the frame.
    #[test]
    fn a_drag_past_the_edge_is_clamped_to_the_frame() {
        let over = Fractions { x: -0.5, y: -0.5, width: 3.0, height: 3.0 };
        assert_eq!(over.to_offsets(640, 480), Some(Rect::new(0, 0, 640, 480)));
    }

    /// The screen mapping is the same drag against the monitor's own
    /// origin — which is not (0, 0) on a second display.
    #[test]
    fn the_screen_rectangle_is_offset_by_the_monitor() {
        let screen = Screen { index: 1, bounds: Rect::new(-1920, 200, 1920, 1080), scale: 1.0 };
        let selection = Fractions { x: 0.5, y: 0.0, width: 0.5, height: 0.5 };
        assert_eq!(
            selection.to_screen(screen),
            Some(Rect::new(-1920 + 960, 200, 960, 540))
        );
    }

    #[test]
    fn a_crop_takes_the_pixels_it_points_at() {
        let cropped = frame(16, 16).crop(Rect::new(4, 8, 4, 2)).expect("inside the frame");
        assert_eq!((cropped.width, cropped.height), (4, 2));
        assert_eq!(&cropped.rgba[..4], &[4, 8, 0, 255]);
        // last pixel of the crop: x = 4 + 4 - 1, y = 8 + 2 - 1
        assert_eq!(&cropped.rgba[cropped.rgba.len() - 4..], &[7, 9, 0, 255]);
    }

    /// A selection is clamped to the frame before it gets here, but a
    /// recording re-captures a rectangle the screen may have changed
    /// under, so a crop that hangs off the edge must trim, not panic.
    #[test]
    fn a_crop_hanging_off_the_edge_is_trimmed() {
        let cropped = frame(8, 8).crop(Rect::new(6, 6, 8, 8)).expect("overlaps");
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(frame(8, 8).crop(Rect::new(20, 20, 4, 4)), None);
    }

    #[test]
    fn a_thumbnail_keeps_the_aspect_ratio_and_averages_the_pixels() {
        let mut shot = frame(400, 200);
        // A solid block, so the average over it is known exactly
        for y in 0..100 {
            for x in 0..200 {
                let i = ((y * 400 + x) * 4) as usize;
                shot.rgba[i..i + 4].copy_from_slice(&[10, 20, 30, 255]);
            }
        }
        let thumb = shot.thumbnail(100);
        assert_eq!((thumb.width, thumb.height), (100, 50));
        assert_eq!(&thumb.rgba[..4], &[10, 20, 30, 255]);
        assert_eq!(thumb.rgba.len(), (100 * 50 * 4) as usize);
    }

    /// Enlarging a frame is not what a thumbnail is for; a small shot is
    /// its own thumbnail.
    #[test]
    fn a_frame_smaller_than_the_limit_is_left_alone() {
        let small = frame(32, 24);
        assert_eq!(small.thumbnail(320), small);
    }

    fn screen(index: usize, x: i32, y: i32, width: u32, height: u32) -> Screen {
        Screen { index, bounds: Rect::new(x, y, width, height), scale: 1.0 }
    }

    /// A second monitor to the left sits at a negative x, and the desktop
    /// that has to hold both starts there.
    #[test]
    fn the_desktop_is_the_rectangle_every_monitor_fits_in() {
        let screens = [
            screen(0, -1920, -120, 1920, 1080),
            screen(1, 0, 0, 2560, 1440),
        ];
        assert_eq!(
            virtual_bounds(&screens),
            Some(Rect::new(-1920, -120, 1920 + 2560, 1440 + 120))
        );
        assert_eq!(virtual_bounds(&[]), None);
    }

    /// Monitors of different sizes at different heights leave gaps, and
    /// the gaps are part of the picture — they are black on the desktop
    /// too.
    #[test]
    fn a_composite_puts_each_monitor_where_it_really_is() {
        let left = Shot::new(2, 2, vec![10; 2 * 2 * 4]);
        let right = Shot::new(2, 4, vec![200; 2 * 4 * 4]);
        let frames = vec![
            (screen(0, -2, 0, 2, 2), left),
            (screen(1, 0, 0, 2, 4), right),
        ];

        let canvas = composite(&frames).expect("two monitors composite");
        assert_eq!((canvas.width, canvas.height), (4, 4));

        let at = |x: u32, y: u32| {
            let i = ((y * canvas.width + x) * 4) as usize;
            canvas.rgba[i..i + 4].to_vec()
        };
        assert_eq!(at(0, 0), vec![10, 10, 10, 10], "the left monitor");
        assert_eq!(at(2, 0), vec![200, 200, 200, 200], "the right monitor");
        // below the short monitor: desktop, not pixels we made up
        assert_eq!(at(0, 3), vec![0, 0, 0, 255]);
    }

    /// One monitor is its own composite; copying it into a canvas would
    /// be a full extra frame in memory for nothing.
    #[test]
    fn a_single_monitor_composites_to_itself() {
        let only = Shot::new(3, 2, vec![7; 3 * 2 * 4]);
        let composed = composite(&[(screen(0, 0, 0, 3, 2), only.clone())]).expect("one monitor");
        assert_eq!(composed, only);
    }

    /// A monitor hanging off the edge of the canvas must be trimmed, not
    /// panic: the desktop is measured before the capture, and a display
    /// can be unplugged in between.
    #[test]
    fn pasting_past_the_edge_is_clipped() {
        let mut canvas = Shot::blank(4, 4);
        canvas.paste(&Shot::new(4, 4, vec![255; 4 * 4 * 4]), 2, 2);
        canvas.paste(&Shot::new(2, 2, vec![128; 2 * 2 * 4]), -1, -1);

        let at = |x: u32, y: u32| {
            let i = ((y * 4 + x) * 4) as usize;
            canvas.rgba[i..i + 4].to_vec()
        };
        assert_eq!(at(3, 3), vec![255, 255, 255, 255]);
        assert_eq!(at(0, 0), vec![128, 128, 128, 128]);
        assert_eq!(at(1, 0), vec![0, 0, 0, 255], "nothing else was touched");
    }

    #[test]
    fn a_frame_survives_a_round_trip_through_png() {
        let shot = frame(37, 21);
        let decoded = decode_png(&shot.to_png().expect("encodes")).expect("decodes");
        assert_eq!(decoded, shot);
    }

    #[test]
    fn a_data_url_announces_itself_as_a_png() {
        let url = frame(4, 4).to_data_url().expect("encodes");
        assert!(url.starts_with("data:image/png;base64,"), "{}", url);
    }
}
