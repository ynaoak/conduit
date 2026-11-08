//! The image editor's side of the app: getting a capture in, and getting
//! the result back out.
//!
//! The editing itself is the tool window's business — a canvas is the
//! right tool for cropping and erasing, and none of it needs Rust. What
//! does need Rust is everything at the edges: reading a capture at full
//! size, writing a PNG where the user chooses, and putting one on the
//! clipboard, none of which a tool window can do on its own.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use tauri::image::Image;
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use crate::capture_history::CaptureStore;
use crate::plugins::workflows::tool_window;

const TOOL_WORKFLOW: &str = "conduit.devtools";
const TOOL_HTML: &str = "image.html";
const GALLERY_HTML: &str = "capture.html";

/// A 4K screenshot is around 10MB of PNG, so this leaves room for a large
/// edited image while still refusing anything that could only be a mistake
/// or an attempt to exhaust memory.
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

fn is_window(label: &str, html: &str) -> bool {
    label == tool_window::window_label(TOOL_WORKFLOW, html)
}

/// The editor, and the launcher itself.
fn may_edit(label: &str) -> bool {
    label == "main" || is_window(label, TOOL_HTML)
}

/// Decode a `data:image/png;base64,…` produced by a canvas.
///
/// Only PNG: it is what `toDataURL` gives by default and the only format
/// that keeps the transparency this tool exists to produce.
fn png_from_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "expected a PNG data URL".to_string())?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "the image is not valid base64".to_string())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("the image is too large".into());
    }
    Ok(bytes)
}

/// A capture at full size, as a data URL the canvas can load.
///
/// The gallery injects thumbnails, which is right for a grid and useless
/// for editing — this is the original.
#[tauri::command]
pub async fn capture_image(
    window: Window,
    id: String,
    store: State<'_, CaptureStore>,
) -> Result<String, String> {
    if !may_edit(window.label()) {
        return Err("this window may not read captures".into());
    }
    let capture = store
        .get(&id)
        .ok_or_else(|| crate::i18n::t("errors", "capture_missing").to_string())?;
    let bytes = std::fs::read(store.path(&capture.file))
        .map_err(|e| format!("could not read the capture: {e}"))?;
    Ok(crate::capture::png_data_url(&bytes))
}

/// Write the edited image where the user says.
#[tauri::command]
pub async fn save_image(
    app: AppHandle,
    window: Window,
    data_url: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    if !may_edit(window.label()) {
        return Err("this window may not write files".into());
    }
    let bytes = png_from_data_url(&data_url)?;

    let name = if suggested_name.trim().is_empty() {
        "image.png".to_string()
    } else {
        suggested_name
    };
    let Some(path) = app
        .dialog()
        .file()
        .set_file_name(&name)
        .add_filter("PNG", &["png"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|e| format!("could not resolve the chosen path: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// Put the edited image on the clipboard.
///
/// Through Rust rather than the canvas: a tool window is served from a
/// custom protocol, and the clipboard image API needs more than that
/// origin reliably has.
#[tauri::command]
pub async fn copy_image(
    app: AppHandle,
    window: Window,
    data_url: String,
) -> Result<(), String> {
    if !may_edit(window.label()) {
        return Err("this window may not use the clipboard".into());
    }
    let bytes = png_from_data_url(&data_url)?;
    let image = Image::from_bytes(&bytes).map_err(|e| format!("not a readable PNG: {e}"))?;
    app.clipboard()
        .write_image(&image)
        .map_err(|e| format!("could not copy the image: {e}"))
}

/// Open the editor on a capture, from the gallery.
///
/// The same handover the launcher's "send to" uses; the gallery is just
/// another place where a picture is already in front of someone.
#[tauri::command]
pub async fn edit_capture(
    app: AppHandle,
    window: Window,
    id: String,
    store: State<'_, CaptureStore>,
) -> Result<(), String> {
    if !(is_window(window.label(), GALLERY_HTML) || window.label() == "main") {
        return Err("this window may not open the editor".into());
    }
    if store.get(&id).is_none() {
        return Err(crate::i18n::t("errors", "capture_missing").to_string());
    }

    app.state::<crate::send_to::PendingInput>().park(
        tool_window::window_label(TOOL_WORKFLOW, TOOL_HTML),
        crate::plugin_system::types::Payload {
            kind: "image".into(),
            value: id,
        },
    );
    tool_window::open(&app, TOOL_WORKFLOW, TOOL_HTML)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_editor_and_the_launcher_touch_images() {
        assert!(may_edit("main"));
        assert!(may_edit(&tool_window::window_label(TOOL_WORKFLOW, TOOL_HTML)));
        // The gallery may ask for the editor to open, but not read or
        // write images itself
        assert!(!may_edit(&tool_window::window_label(TOOL_WORKFLOW, GALLERY_HTML)));
        assert!(!may_edit(&tool_window::window_label("imported.package", TOOL_HTML)));
    }

    #[test]
    fn a_canvas_data_url_decodes_to_its_bytes() {
        // 1x1 transparent PNG, as `canvas.toDataURL()` writes one
        let png = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
            .unwrap();
        let url = format!("data:image/png;base64,{}", STANDARD.encode(&png));
        assert_eq!(png_from_data_url(&url).unwrap(), png);
    }

    /// Anything that is not a PNG data URL is refused before it reaches a
    /// decoder or the filesystem.
    #[test]
    fn other_shapes_are_refused() {
        for text in [
            "",
            "data:image/jpeg;base64,AAAA",
            "data:text/html;base64,AAAA",
            "https://example.com/x.png",
            "data:image/png;base64,not base64!",
        ] {
            assert!(png_from_data_url(text).is_err(), "{text}");
        }
    }
}
