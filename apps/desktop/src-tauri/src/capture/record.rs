//! Recording a rectangle of the screen.
//!
//! The two platforms record differently, and — as with the color picker —
//! the difference is not an implementation detail:
//!
//! * **Windows** has nothing to record with, so the session is ours:
//!   the same `BitBlt` a screenshot uses, ten times a second, encoded
//!   straight into an animated GIF. Streaming into the file as frames
//!   arrive is what keeps a minute of video from being a gigabyte of
//!   `Vec<u8>` first.
//! * **macOS** has `screencapture -v`, which writes a real QuickTime
//!   movie and, more importantly, is the path the OS knows how to ask for
//!   Screen Recording consent on. Ctrl-C is how that tool is stopped, so
//!   stopping the recording is an interrupt to the child.
//!
//! Either way the recorder puts one small window on screen — a clock and
//! a stop button — because a recording with no visible state is a
//! recording people leave running.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

use super::{Rect, Screen, Shot};
use crate::capture_history::{now_ms, Capture, CaptureStore};

pub const LABEL: &str = "capture-recorder";

/// Ten frames a second: enough for a UI walkthrough, and slow enough that
/// quantising each frame keeps up on one thread.
#[cfg(windows)]
const FPS: u32 = 10;

/// A recording nobody stops ends on its own. Two minutes of a screen is
/// already a big file, and an unbounded one is a disk filling up behind a
/// window the user forgot about.
pub const MAX_SECONDS: u64 = 120;

/// Longest edge of a recorded frame. Quantising 4K frames at 10fps is not
/// something one thread does in real time, and a screen recording is
/// nearly always looked at smaller than life anyway.
#[cfg(windows)]
const MAX_EDGE: u32 = 1280;

/// The window's size, in logical pixels, and how far above the bottom of
/// the screen it floats.
const CONTROL_WIDTH: f64 = 232.0;
const CONTROL_HEIGHT: f64 = 64.0;
const CONTROL_MARGIN: f64 = 48.0;

struct Running {
    stop: Arc<AtomicBool>,
    done: Option<oneshot::Receiver<Result<Capture, String>>>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

pub fn is_running() -> bool {
    RUNNING.lock().map(|r| r.is_some()).unwrap_or(false)
}

/// True where recording is implemented, which is the same two platforms
/// that can capture a still — for different reasons on each.
pub const fn is_supported() -> bool {
    cfg!(windows) || cfg!(target_os = "macos")
}

/// What a finished recording is called. The Windows session encodes a GIF
/// itself; macOS hands back what `screencapture` wrote.
pub const fn extension() -> &'static str {
    if cfg!(target_os = "macos") {
        "mov"
    } else {
        "gif"
    }
}

/// Start recording `area` (screen coordinates, physical pixels).
///
/// `poster` is the still already taken of that rectangle: a video has no
/// thumbnail we could read back without decoding it, and the frame the
/// recording starts on is the honest one to show.
pub async fn start(
    app: &AppHandle,
    store: &CaptureStore,
    area: Rect,
    screen: Screen,
    poster: Shot,
) -> Result<(), String> {
    if !is_supported() {
        return Err(crate::i18n::t("errors", "record_unsupported").to_string());
    }
    if is_running() {
        return Err(crate::i18n::t("errors", "record_busy").to_string());
    }

    let created_ms = now_ms();
    let (id, path) = store.reserve_video(created_ms, extension());

    let stop = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = oneshot::channel();
    {
        let mut running = RUNNING.lock().map_err(|_| "recorder state is poisoned")?;
        *running = Some(Running {
            stop: stop.clone(),
            done: Some(receiver),
        });
    }

    open_controls(app, screen).await?;

    let app = app.clone();
    let store = store.clone();
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();

    // A std thread rather than a task: the work is a blocking capture
    // loop (or a blocking wait on a child process), and it runs for as
    // long as the recording does.
    std::thread::spawn(move || {
        let outcome = run(&path, area, screen.scale, &stop).and_then(|seconds| {
            store.add_video(id, file, &poster, seconds, created_ms)
        });
        if outcome.is_err() {
            let _ = std::fs::remove_file(&path);
        }

        // Clear before the window closes, so the state a user can see and
        // the state the commands check never disagree.
        if let Ok(mut running) = RUNNING.lock() {
            *running = None;
        }
        close_controls(&app);
        // Nobody is listening when the recording stopped by itself, and
        // the result is already in the history by then.
        let _ = sender.send(outcome);
    });

    Ok(())
}

/// Stop the recording and wait for the file to be finished and filed.
pub async fn stop() -> Result<Capture, String> {
    let (stop, done) = {
        let mut running = RUNNING.lock().map_err(|_| "recorder state is poisoned")?;
        let Some(session) = running.as_mut() else {
            return Err(crate::i18n::t("errors", "record_none").to_string());
        };
        (session.stop.clone(), session.done.take())
    };
    stop.store(true, Ordering::SeqCst);

    let done = done.ok_or_else(|| crate::i18n::t("errors", "record_none").to_string())?;
    done.await
        .map_err(|_| crate::i18n::t("errors", "record_lost").to_string())?
}

/// Seconds recorded, or why not.
fn run(path: &std::path::Path, area: Rect, scale: f64, stop: &AtomicBool) -> Result<f64, String> {
    #[cfg(target_os = "macos")]
    {
        return run_screencapture(path, area, scale, stop);
    }
    #[cfg(windows)]
    {
        let _ = scale;
        return run_gif(path, area, stop);
    }
    #[allow(unreachable_code)]
    {
        let _ = (path, area, scale, stop);
        Err(crate::i18n::t("errors", "record_unsupported").to_string())
    }
}

/// The Windows session: capture, quantise, append, repeat.
#[cfg(windows)]
fn run_gif(path: &std::path::Path, area: Rect, stop: &AtomicBool) -> Result<f64, String> {
    use std::io::BufWriter;
    use std::time::{Duration, Instant};

    use super::capture;

    let first = capture(area, 1.0)?.thumbnail(MAX_EDGE);
    let (width, height) = (first.width as u16, first.height as u16);

    let file = std::fs::File::create(path).map_err(|e| format!("failed to open {:?}: {}", path, e))?;
    let mut encoder = gif::Encoder::new(BufWriter::new(file), width, height, &[])
        .map_err(|e| format!("failed to start the recording: {}", e))?;
    encoder
        .set_repeat(gif::Repeat::Infinite)
        .map_err(|e| format!("failed to start the recording: {}", e))?;

    let started = Instant::now();
    let budget = Duration::from_micros(1_000_000 / FPS as u64);
    // A frame is written once the next one has been taken, because its
    // delay is how long it was actually on screen — measured, not assumed.
    // Encoding that falls behind then slows the GIF down to match reality
    // instead of playing it back too fast.
    let mut pending = Some((first, Instant::now()));

    while !stop.load(Ordering::SeqCst) && started.elapsed().as_secs() < MAX_SECONDS {
        let frame_start = Instant::now();
        let shot = match capture(area, 1.0) {
            Ok(shot) => shot.thumbnail(MAX_EDGE),
            // A monitor that went away mid-recording ends the recording;
            // what was captured so far is still worth keeping.
            Err(_) => break,
        };
        if let Some((previous, taken)) = pending.take() {
            write_frame(&mut encoder, previous, taken.elapsed())?;
        }
        pending = Some((shot, frame_start));

        if let Some(remaining) = budget.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(remaining);
        }
    }

    if let Some((last, taken)) = pending {
        write_frame(&mut encoder, last, taken.elapsed())?;
    }
    drop(encoder);
    Ok(started.elapsed().as_secs_f64())
}

#[cfg(windows)]
fn write_frame<W: std::io::Write>(
    encoder: &mut gif::Encoder<W>,
    shot: Shot,
    on_screen: std::time::Duration,
) -> Result<(), String> {
    let mut rgba = shot.rgba;
    // speed 30 of 1..=30: the fastest quantiser. A screen recording is
    // flat colour and text, which NeuQuant handles well even in a hurry,
    // and the alternative is dropping frames.
    let mut frame = gif::Frame::from_rgba_speed(shot.width as u16, shot.height as u16, &mut rgba, 30);
    // GIF delays are hundredths of a second, and 0 means "as fast as the
    // viewer likes" — which is not what a 100ms frame meant.
    frame.delay = ((on_screen.as_millis() / 10) as u16).max(1);
    encoder
        .write_frame(&frame)
        .map_err(|e| format!("failed to write a frame: {}", e))
}

/// The macOS session: hand the rectangle to `screencapture -v` and wait.
///
/// Untested on a Mac from here (this repository cannot build for macOS),
/// so it deliberately does the least it can: no frame loop of our own, no
/// encoder, and the stop is the interrupt the tool documents.
#[cfg(target_os = "macos")]
fn run_screencapture(
    path: &std::path::Path,
    area: Rect,
    scale: f64,
    stop: &AtomicBool,
) -> Result<f64, String> {
    use std::process::Command;
    use std::time::{Duration, Instant};

    let scale = if scale > 0.0 { scale } else { 1.0 };
    let region = format!(
        "{},{},{},{}",
        area.x as f64 / scale,
        area.y as f64 / scale,
        area.width as f64 / scale,
        area.height as f64 / scale
    );

    let started = Instant::now();
    let mut child = Command::new(super::macos::tool())
        .args(["-v", "-x", "-R", &region, "-V", &MAX_SECONDS.to_string()])
        .arg(path)
        .spawn()
        .map_err(|e| format!("failed to run screencapture: {}", e))?;

    // The child stops itself at -V seconds; polling both means the window
    // and the state clear at the same moment either way.
    while !stop.load(Ordering::SeqCst) {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(started.elapsed().as_secs_f64()),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("failed to watch screencapture: {}", e)),
        }
    }

    // SIGINT, not kill: the tool finalises the movie on an interrupt and
    // leaves an unplayable file if it is killed outright.
    let _ = Command::new("/bin/kill")
        .args(["-INT", &child.id().to_string()])
        .status();
    let _ = child.wait();
    Ok(started.elapsed().as_secs_f64())
}

/// The recorder's own window: elapsed time and a stop button, floating
/// over the bottom of the screen being recorded.
async fn open_controls(app: &AppHandle, screen: Screen) -> Result<(), String> {
    let (done, built) = oneshot::channel();
    let handle = app.clone();

    app.run_on_main_thread(move || {
        let result = WebviewWindowBuilder::new(
            &handle,
            LABEL,
            WebviewUrl::App("index.html#recorder".into()),
        )
        .title("Conduit recorder")
        .inner_size(CONTROL_WIDTH, CONTROL_HEIGHT)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .build()
        .map_err(|e| e.to_string())
        .map(|window| {
            let scale = if screen.scale > 0.0 { screen.scale } else { 1.0 };
            let width = (CONTROL_WIDTH * scale) as i32;
            let height = (CONTROL_HEIGHT * scale) as i32;
            let x = screen.bounds.x + (screen.bounds.width as i32 - width) / 2;
            let y = screen.bounds.y + screen.bounds.height as i32
                - height
                - (CONTROL_MARGIN * scale) as i32;
            let _ = window.set_position(PhysicalPosition::new(x, y));
        });
        let _ = done.send(result);
    })
    .map_err(|e| e.to_string())?;

    built
        .await
        .map_err(|_| "the recorder controls never opened".to_string())?
}

fn close_controls(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing recorded, nothing to stop — and the error has to be the
    /// user-facing one, since this is reachable from the recorder window
    /// clicking stop twice.
    #[tokio::test]
    async fn stopping_without_a_recording_says_so() {
        assert!(!is_running());
        assert!(stop().await.is_err());
    }

    /// The extension decides how the gallery shows a recording (an <img>
    /// for a GIF, a <video> for a movie), so it has to follow the platform
    /// that actually produced the file.
    #[test]
    fn the_extension_matches_what_the_platform_writes() {
        if cfg!(target_os = "macos") {
            assert_eq!(extension(), "mov");
        } else {
            assert_eq!(extension(), "gif");
        }
    }
}
