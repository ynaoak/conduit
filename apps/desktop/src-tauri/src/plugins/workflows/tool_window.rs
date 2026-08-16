//! HTML apps from workflow packages, opened in their own webview window.
//!
//! Files are served through the `conduit-wf` custom protocol, which only
//! reads from the installed workflows directory. Windows are undecorated
//! and transparent so they match the launcher's rounded card; the drag
//! region and close button are injected into every document served here,
//! so third-party packages get working chrome without knowing about it.
//!
//! Capabilities for `tool-*` labels grant dragging, maximize-toggle and
//! closing the window itself and nothing else, so a package still cannot
//! reach the filesystem, the shell, or any other Tauri command.

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::config::ConfigState;
use crate::window::{RenderedTheme, ResolvedTheme};

use super::{load_workflows, workflows_dir};

/// Custom protocol name; registered in lib.rs
pub const SCHEME: &str = "conduit-wf";

/// Default tool window size, in logical pixels: +32px each way for the
/// gutter the chrome adds, so the visible card keeps the 520x640 it had
/// with an OS frame.
const DEFAULT_WIDTH: f64 = 552.0;
const DEFAULT_HEIGHT: f64 = 672.0;
/// Small enough for a converter, large enough that the chrome bar and a
/// line of content still fit — below this a window is only confusing.
const MIN_WIDTH: f64 = 392.0;
const MIN_HEIGHT: f64 = 452.0;
/// A window bigger than this is a manifest typo, not a layout: it would
/// open past the edge of any display it was launched on.
const MAX_WIDTH: f64 = 2400.0;
const MAX_HEIGHT: f64 = 1600.0;

/// The size a tool window opens at. A package may ask for more room; the
/// bounds are ours, since the package cannot know what it is opening on.
fn window_size(width: Option<f64>, height: Option<f64>) -> (f64, f64) {
    let sane = |asked: Option<f64>, default: f64, min: f64, max: f64| {
        asked
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(min, max))
            .unwrap_or(default)
    };
    (
        sane(width, DEFAULT_WIDTH, MIN_WIDTH, MAX_WIDTH),
        sane(height, DEFAULT_HEIGHT, MIN_HEIGHT, MAX_HEIGHT),
    )
}

/// Directory-name rule shared with manifest validation
fn valid_workflow_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Package-relative asset path: forward slashes only, no absolute paths,
/// no drive letters, and no `.`/`..` components. Applied both when opening
/// a window (path from a result id) and when serving a request (path from
/// the webview), so a crafted request cannot escape the workflows dir.
pub fn sanitize_rel_path(rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\\') || rel.contains(':') || rel.starts_with('/') {
        return None;
    }
    let mut path = PathBuf::new();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// The URL a tool window navigates to. Always the canonical
/// `<scheme>://localhost/` form: wry itself rewrites navigation URLs of
/// registered protocols to the platform representation (on Windows,
/// `http://<scheme>.localhost/`), so hand-building the Windows form here
/// would just be a second, driftable copy of that mapping.
/// The launcher's appearance rides along as a query parameter so tool
/// windows can match it — they have no IPC access, so the URL is the only
/// channel. "system" (only used before the webview has reported) keeps the
/// app on prefers-color-scheme.
fn tool_url(workflow_id: &str, rel: &str, theme_mode: &str) -> String {
    format!(
        "{}://localhost/{}/{}?theme={}",
        SCHEME, workflow_id, rel, theme_mode
    )
}

/// Open (or re-focus) the window for an HTML app.
pub async fn open(app: &AppHandle, workflow_id: &str, html: &str) -> anyhow::Result<()> {
    if !valid_workflow_id(workflow_id) {
        anyhow::bail!("invalid workflow id: {}", workflow_id);
    }
    let rel = sanitize_rel_path(html)
        .ok_or_else(|| anyhow::anyhow!("invalid html path: {}", html))?;

    let dir = workflows_dir(app)?;
    let file = dir.join(workflow_id).join(&rel);
    if !file.is_file() {
        anyhow::bail!(crate::i18n::tf(
            "errors",
            "tool_window_missing",
            &[("path", &format!("{}/{}", workflow_id, html))]
        ));
    }

    // Window title and size: the app's manifest name, falling back to the
    // file name
    let workflows = load_workflows(&dir);
    let definition = workflows
        .iter()
        .find(|w| w.id == workflow_id)
        .and_then(|w| w.apps.iter().find(|a| a.html.as_deref() == Some(html)));
    let (width, height) = window_size(
        definition.and_then(|app| app.width),
        definition.and_then(|app| app.height),
    );
    let title = definition
        .map(|app| app.name.clone())
        .unwrap_or_else(|| html.to_string());

    // One window per app: a second launch focuses the existing window
    let label = window_label(workflow_id, html);

    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    // What the launcher renders wins over theme.mode: color overrides can
    // make a "system" config look dark on a light OS, and the tool window
    // must match the window the user launched it from.
    let theme_mode = match app.state::<ResolvedTheme>().get() {
        Some(resolved) => resolved.mode,
        None => app.state::<ConfigState>().get().await.theme.mode,
    };
    let url: tauri::Url = tool_url(workflow_id, html, &theme_mode)
        .parse()
        .map_err(|e| anyhow::anyhow!("bad tool url: {}", e))?;

    // Window creation must happen on the main thread on some platforms;
    // execute() runs on the async runtime. The result travels back so a
    // failure surfaces as an error toast instead of the launcher quietly
    // hiding over a window that never appeared.
    let (done, built) = tokio::sync::oneshot::channel();
    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        let result = tauri::WebviewWindowBuilder::new(
            &app_handle,
            &label,
            tauri::WebviewUrl::External(url),
        )
        .title(&title)
        .inner_size(width, height)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        // Match the launcher: no OS title bar, transparent so the CSS
        // rounded corners are real (an opaque window paints square ones)
        .decorations(false)
        .transparent(true)
        // Without this the window can open behind whatever was in front.
        // macOS in particular will not raise a window for an app that is
        // not already frontmost, and the launcher hides itself right after
        // launching, so nothing else was going to bring it forward.
        .focused(true)
        .build();
        let result = result.map(|window| {
            // The builder flag alone is not always enough once the app has
            // lost activation; asking the finished window costs nothing.
            let _ = window.set_focus();
        });
        let _ = done.send(result.map_err(|e| e.to_string()));
    })?;

    match built.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(anyhow::anyhow!(crate::i18n::tf(
            "errors",
            "tool_window_open",
            &[("error", &e)]
        ))),
        // The closure was dropped without running (shutting down)
        Err(_) => Err(anyhow::anyhow!(crate::i18n::t(
            "errors",
            "tool_window_interrupted"
        ))),
    }
}

/// Window chrome injected into every top-level HTML document served to a
/// tool window: the windows are undecorated, so the drag strip and the
/// close button have to come from somewhere. Injecting at serve time means
/// an imported package needs no knowledge of it.
const CHROME: &str = r##"<style id="__conduit-chrome-style">
  /* Tool windows are undecorated and transparent (see tool_window.rs), so
     the rounded card, its gutter and the chrome are drawn here — the same
     shape the launcher has. */
  html {
    background-color: transparent;
    /* An app paints its surface on <body>, and CSS propagates that to the
       canvas when the root has no background of its own — which fills the
       gutter and defeats the transparent window. A background-image, even
       a fully transparent one, gives the root a background and stops the
       propagation. */
    background-image: linear-gradient(rgba(0, 0, 0, 0), rgba(0, 0, 0, 0));
    box-sizing: border-box;
    height: 100%;
    /* room for the elevation shadow, which clips at the window edge */
    padding: 12px 16px 20px;
    overflow: hidden;
  }
  body {
    position: relative; /* containing block for the chrome bar */
    height: 100%;
    margin: 0;
    border-radius: var(--conduit-radius, 28px);
    /* inset ring instead of a border: no layout shift for the app */
    box-shadow: inset 0 0 0 1px var(--conduit-outline, rgba(128, 128, 128, 0.35)),
      0 4px 8px 3px rgba(0, 0, 0, 0.15), 0 1px 3px rgba(0, 0, 0, 0.3);
  }
  #__conduit-chrome {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 38px;
    z-index: 2147483647;
    display: flex;
    align-items: center;
    /* set per platform by __conduit-platform below */
    justify-content: var(--conduit-chrome-side, flex-end);
    user-select: none;
    -webkit-user-select: none;
  }
  /* the launcher's grabber pill, centered */
  #__conduit-chrome::before {
    content: "";
    position: absolute;
    top: 17px;
    left: 50%;
    transform: translateX(-50%);
    width: 48px;
    height: 4px;
    border-radius: 2px;
    background: var(--conduit-outline, rgba(128, 128, 128, 0.5));
    pointer-events: none;
  }
  #__conduit-chrome button {
    background: transparent;
    border: none;
    border-radius: 999px;
    color: var(--conduit-on-variant, #8a8a8a);
    cursor: pointer;
    font: 13px/1 system-ui, sans-serif;
    margin: var(--conduit-chrome-button-margin, 0 6px 0 0);
    padding: 5px 8px;
  }
  #__conduit-chrome button:hover {
    background: var(--conduit-surface-high, rgba(128, 128, 128, 0.22));
    color: var(--conduit-on-surface, inherit);
  }

  /* Resize grips. An undecorated window has no frame to grab, so the
     edges are ours to provide — a band along each side of the gutter,
     with the corners on top of them. Invisible on purpose: what says
     "you can resize here" is the cursor. */
  .__conduit-grip {
    position: fixed;
    z-index: 2147483647;
    /* The window is transparent outside the card; a grip must not paint
       over the desktop, only listen there. */
    background: transparent;
  }
</style>
<script>
(function () {
  // Only the top-level document gets chrome; an iframe keeps the host's.
  if (window.self !== window.top) return;

  var BAR_HEIGHT = 38;
  // Strings for the launcher's language, injected above. Absent when the
  // file is opened outside conduit, in which case the markup's own text
  // stands as written.
  var S = window.__conduitStrings || {};

  // Any element the app tags gets translated — third-party packages opt in
  // by adding the attributes, and need no code of their own.
  document.querySelectorAll("[data-i18n]").forEach(function (el) {
    var value = S[el.getAttribute("data-i18n")];
    if (value) el.textContent = value;
  });
  document.querySelectorAll("[data-i18n-placeholder]").forEach(function (el) {
    var value = S[el.getAttribute("data-i18n-placeholder")];
    if (value) el.setAttribute("placeholder", value);
  });

  function closeWindow() {
    // The same internal entry point Tauri's own drag-region script uses —
    // tool windows do not load the JS API bundle. Permitted for tool-*
    // labels only (capabilities/tool-window.json).
    if (window.__TAURI_INTERNALS__) {
      window.__TAURI_INTERNALS__.invoke("plugin:window|close");
    }
  }

  // The gutter around the card is window too, and aiming for the edge of
  // what you can see is the natural thing to do. Non-deep, so this only
  // fires when the padding itself is the target — body covers the rest.
  document.documentElement.setAttribute("data-tauri-drag-region", "");
  document.documentElement.style.cursor = "grab";

  var bar = document.createElement("div");
  bar.id = "__conduit-chrome";
  // "deep" so the whole strip drags; the close button blocks it by being a
  // clickable element, which is what Tauri's drag script already handles.
  bar.setAttribute("data-tauri-drag-region", "deep");

  var close = document.createElement("button");
  close.type = "button";
  close.title = S.close || "Close (Esc)";
  close.setAttribute("aria-label", S.close || "Close");
  close.textContent = "✕";
  close.addEventListener("click", closeWindow);
  bar.appendChild(close);
  document.body.appendChild(bar);

  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") closeWindow();
  });

  // Eight grips around the edge. Tauri has a drag-region attribute but no
  // resize equivalent, so the direction is passed to the same internal
  // IPC the JS API's startResizeDragging() uses. Permitted for tool-*
  // labels only (capabilities/tool-window.json).
  var EDGE = 6;      // how wide a band is, in CSS pixels
  var CORNER = 14;   // corners win over edges, and are easier to hit
  var GRIPS = [
    ["North", "ns-resize", { top: 0, left: 0, right: 0, height: EDGE }],
    ["South", "ns-resize", { bottom: 0, left: 0, right: 0, height: EDGE }],
    ["West", "ew-resize", { top: 0, bottom: 0, left: 0, width: EDGE }],
    ["East", "ew-resize", { top: 0, bottom: 0, right: 0, width: EDGE }],
    ["NorthWest", "nwse-resize", { top: 0, left: 0, width: CORNER, height: CORNER }],
    ["NorthEast", "nesw-resize", { top: 0, right: 0, width: CORNER, height: CORNER }],
    ["SouthWest", "nesw-resize", { bottom: 0, left: 0, width: CORNER, height: CORNER }],
    ["SouthEast", "nwse-resize", { bottom: 0, right: 0, width: CORNER, height: CORNER }],
  ];

  GRIPS.forEach(function (grip) {
    var direction = grip[0];
    var element = document.createElement("div");
    element.className = "__conduit-grip";
    element.style.cursor = grip[1];
    Object.keys(grip[2]).forEach(function (side) {
      element.style[side] = grip[2][side] + "px";
    });
    element.addEventListener("mousedown", function (e) {
      // Left button only: a right-click here is a context menu, not a
      // resize the user cannot see the end of.
      if (e.button !== 0 || !window.__TAURI_INTERNALS__) return;
      e.preventDefault();
      e.stopPropagation();
      window.__TAURI_INTERNALS__.invoke("plugin:window|start_resize_dragging", {
        value: direction,
      });
    });
    document.body.appendChild(element);
  });

  // Reserve room for the fixed bar on top of whatever padding the app
  // already has, rather than replacing it.
  var padding = parseFloat(getComputedStyle(document.body).paddingTop) || 0;
  document.body.style.paddingTop = padding + BAR_HEIGHT + "px";

  // A transparent window shows the desktop wherever nothing is painted.
  // Apps that set their own background keep it; the rest get the
  // launcher's surface so they are not see-through.
  var background = getComputedStyle(document.body).backgroundColor;
  if (!background || background === "transparent" ||
      background.replace(/\s/g, "") === "rgba(0,0,0,0)") {
    document.body.style.backgroundColor =
      getComputedStyle(document.documentElement)
        .getPropertyValue("--conduit-surface").trim() || "Canvas";
  }
})();
</script>
"##;

/// Byte index of the first ASCII-case-insensitive occurrence of `needle`.
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Byte index of the last ASCII-case-insensitive occurrence of `needle`.
/// Byte comparison is safe for a UTF-8 haystack because an ASCII byte
/// never appears inside a multi-byte sequence.
fn rfind_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).rev().find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// The window label for one app of one package.
///
/// Derived rather than stored so the same app always lands on the same
/// window, and so a caller holding only a label can tell which package is
/// in it — which is what gates commands that not every package may use.
pub fn window_label(workflow_id: &str, html: &str) -> String {
    let mut hasher = DefaultHasher::new();
    (workflow_id, html).hash(&mut hasher);
    format!("tool-{:016x}", hasher.finish())
}

/// The launcher's rendered palette as `--conduit-*` custom properties.
/// Apps read these (with their own values as fallbacks), which is what
/// keeps a tool window the same color as the launcher even when
/// config.json overrides slots. Empty until the webview has reported —
/// the fallbacks cover that window.
/// Where the tool window's own close button sits.
///
/// macOS puts window controls at the top left and Windows at the top right,
/// and a window that gets this wrong is the kind of thing you notice before
/// you notice anything else about it. Decided here rather than in the
/// injected script because the platform is known at compile time.
fn platform_css() -> &'static str {
    if cfg!(target_os = "macos") {
        "<style id=\"__conduit-platform\">:root{--conduit-chrome-side:flex-start;\
         --conduit-chrome-button-margin:0 0 0 13px;}</style>"
    } else {
        "<style id=\"__conduit-platform\">:root{--conduit-chrome-side:flex-end;\
         --conduit-chrome-button-margin:0 6px 0 0;}</style>"
    }
}

/// The macOS close button is the system traffic light: a 12px red disc
/// that shows its glyph only while the pointer is over it.
///
/// Injected after CHROME rather than alongside the platform variables,
/// which go in the head — CHROME's own button rules come at the end of the
/// body and would win at equal specificity from there.
///
/// Takes the platform as an argument so the macOS sheet can be inspected
/// from a test on any host; the build cannot even be compiled for macOS
/// here, so a `cfg!` inside would make it untestable.
fn traffic_light_css(macos: bool) -> &'static str {
    if !macos {
        return "";
    }
    r##"<style id="__conduit-traffic-light">
  #__conduit-chrome button {
    position: relative;
    width: 12px;
    height: 12px;
    padding: 0;
    border-radius: 50%;
    background: #ff5f57;
    /* the system disc has a slightly darker rim rather than a flat edge */
    box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.15);
    /* the markup keeps its own glyph so a tool window opened without this
       sheet still has a visible button; the disc draws a thinner one */
    font-size: 0;
    color: transparent;
  }
  /* The disc keeps its color under the pointer — what changes is the
     glyph, which macOS reveals only while the pointer is over it. */
  #__conduit-chrome button:hover {
    background: #ff5f57;
  }
  #__conduit-chrome button:active {
    background: #bf4942;
  }
  #__conduit-chrome button::before,
  #__conduit-chrome button::after {
    content: "";
    position: absolute;
    left: 3px;
    top: 5.5px;
    width: 6px;
    height: 1px;
    background: rgba(0, 0, 0, 0.55);
    opacity: 0;
    transition: opacity 120ms ease;
  }
  #__conduit-chrome button::before { transform: rotate(45deg); }
  #__conduit-chrome button::after { transform: rotate(-45deg); }
  #__conduit-chrome button:hover::before,
  #__conduit-chrome button:hover::after { opacity: 1; }
</style>"##
}

fn palette_css(theme: Option<&RenderedTheme>) -> String {
    let Some(theme) = theme.filter(|t| !t.colors.is_empty()) else {
        return String::new();
    };
    let vars: String = theme
        .colors
        .iter()
        .map(|(name, value)| format!("--conduit-{}:{};", name, value))
        .collect();
    format!("<style id=\"__conduit-palette\">:root{{{}}}</style>", vars)
}

/// The chrome bar, plus the platform sheet that restyles its button. The
/// sheet has to follow CHROME: both target `#__conduit-chrome button`, so
/// whichever comes last wins.
fn chrome_html(macos: bool) -> String {
    format!("{}{}", CHROME, traffic_light_css(macos))
}

/// Place the chrome just before `</body>` so the script runs with the body
/// already parsed; documents without a closing tag get it appended.
/// Tool-app strings for the launcher's language. The chrome script uses
/// them for its own labels and applies them to any `data-i18n` element,
/// so an app is translated without shipping a copy of the catalog.
fn strings_js() -> String {
    format!(
        "<script id=\"__conduit-strings\">window.__conduitStrings={};</script>",
        crate::i18n::section_json("tools")
    )
}

/// The picked-color history, for the built-in Color Palette app.
///
/// Only built-in packages get it. The colors someone sampled off their
/// screen are theirs, and an imported package is third-party code — it can
/// already be handed nothing but its own files, and that is the right
/// default to keep.
/// `history` is only called once the package has earned the data, which
/// also keeps it out of the request path for every other package.
fn colors_js(workflow_id: &str, history: impl FnOnce() -> String) -> String {
    if !super::builtin::is_builtin(workflow_id) {
        return String::new();
    }
    format!(
        "<script id=\"__conduit-colors\">window.__conduitColors={};</script>",
        history()
    )
}

/// The capture history, for the built-in gallery app — the list, and each
/// preview inlined as a data URL, since a tool window cannot read a file.
///
/// Built-in only, for the same reason the colors are: screenshots of
/// whatever the user had open are about as personal as this app gets, and
/// an imported package is third-party code that gets its own files and
/// nothing else.
fn captures_js(workflow_id: &str, history: impl FnOnce() -> String) -> String {
    if !super::builtin::is_builtin(workflow_id) {
        return String::new();
    }
    format!(
        "<script id=\"__conduit-captures\">window.__conduitCaptures={};</script>",
        history()
    )
}

/// Palette and strings go in the head, ahead of the app's own scripts —
/// an app reads `window.__conduitStrings` while it parses, so injecting
/// them at the end of the body would hand it an empty catalog. The chrome
/// itself goes last, where the body it appends to already exists.
fn inject_chrome(html: &str, palette: &str, strings: &str) -> String {
    let head = format!("{}{}", palette, strings);
    let with_head = match find_ascii_ci(html, "</head>") {
        Some(i) => format!("{}{}{}", &html[..i], head, &html[i..]),
        // No head to speak of: still ahead of everything else
        None => format!("{}{}", head, html),
    };
    let chrome = chrome_html(cfg!(target_os = "macos"));
    match rfind_ascii_ci(&with_head, "</body>") {
        Some(i) => format!("{}{}{}", &with_head[..i], chrome, &with_head[i..]),
        None => format!("{}{}", with_head, chrome),
    }
}

fn is_html(path: &PathBuf) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "html" | "htm"
    )
}

fn mime_for(path: &PathBuf) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" | "md" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Serve `conduit-wf://localhost/<workflow_id>/<path>` from the workflows
/// directory. Anything outside it — bad ids, traversal, unknown files —
/// gets a 404.
pub fn serve(
    app: &AppHandle,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Cow<'static, [u8]>> {
    fn not_found() -> tauri::http::Response<Cow<'static, [u8]>> {
        tauri::http::Response::builder()
            .status(404)
            .body(Cow::Borrowed(&b""[..]))
            .expect("static response")
    }

    let raw_path = request.uri().path();
    let decoded = match urlencoding::decode(raw_path) {
        Ok(d) => d.into_owned(),
        Err(_) => return not_found(),
    };
    let Some((workflow_id, rel)) = decoded.trim_start_matches('/').split_once('/') else {
        return not_found();
    };
    if !valid_workflow_id(workflow_id) {
        return not_found();
    }
    let Some(rel_path) = sanitize_rel_path(rel) else {
        return not_found();
    };
    let Ok(dir) = workflows_dir(app) else {
        return not_found();
    };

    let file = dir.join(workflow_id).join(&rel_path);
    let Ok(bytes) = std::fs::read(&file) else {
        return not_found();
    };

    // Non-UTF-8 HTML is served untouched rather than mangled by injection
    // The platform sheet goes with the palette because both belong in the
    // head, ahead of the app's own styles; unlike the palette it is not
    // optional, so it is concatenated rather than replaced.
    let palette = format!(
        "{}{}",
        platform_css(),
        palette_css(app.state::<ResolvedTheme>().get().as_ref())
    );
    let head_scripts = format!(
        "{}{}{}",
        strings_js(),
        colors_js(workflow_id, || {
            app.state::<crate::color_history::ColorHistoryState>().as_hex_json()
        }),
        captures_js(workflow_id, || {
            app.state::<crate::capture_history::CaptureStore>().as_gallery_json()
        })
    );
    let bytes = match (is_html(&file), String::from_utf8(bytes)) {
        (true, Ok(html)) => inject_chrome(&html, &palette, &head_scripts).into_bytes(),
        (_, Ok(other)) => other.into_bytes(),
        (_, Err(e)) => e.into_bytes(),
    };

    // Imported packages are third-party code: no network, no navigation
    // away, everything from the package itself (inline allowed so
    // single-file tools work)
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", mime_for(&file))
        .header(
            "Content-Security-Policy",
            "default-src 'self'; script-src 'self' 'unsafe-inline'; \
             style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
             connect-src 'none'; form-action 'none'",
        )
        .body(Cow::Owned(bytes))
        .expect("static response")
}

#[cfg(test)]
mod size_tests {
    use super::*;

    /// A package asks for room; the bounds are ours. A window that opens
    /// 8000px wide is off the side of any display it was launched on, and
    /// one that opens 40px tall has no chrome bar left.
    #[test]
    fn a_requested_size_is_honoured_within_reason() {
        assert_eq!(window_size(None, None), (DEFAULT_WIDTH, DEFAULT_HEIGHT));
        assert_eq!(window_size(Some(900.0), Some(780.0)), (900.0, 780.0));
        assert_eq!(window_size(Some(8000.0), Some(9000.0)), (MAX_WIDTH, MAX_HEIGHT));
        assert_eq!(window_size(Some(40.0), Some(0.0)), (MIN_WIDTH, MIN_HEIGHT));
        // A manifest written by hand can carry anything at all
        assert_eq!(window_size(Some(f64::NAN), Some(f64::INFINITY)), (DEFAULT_WIDTH, DEFAULT_HEIGHT));
    }

    /// The grips are the only way to resize an undecorated window, so
    /// every direction has to reach the IPC that does it.
    #[test]
    fn every_edge_and_corner_can_resize() {
        let chrome = chrome_html(false);
        for direction in [
            "North", "South", "East", "West", "NorthEast", "NorthWest", "SouthEast", "SouthWest",
        ] {
            assert!(chrome.contains(&format!("\"{}\"", direction)), "no {} grip", direction);
        }
        assert!(chrome.contains("plugin:window|start_resize_dragging"));
    }
}

#[cfg(test)]
mod tests {
    /// Window controls sit on opposite sides on macOS and Windows, and a
    /// window with them in the wrong corner reads as foreign immediately.
    #[test]
    fn the_close_button_follows_the_platform_convention() {
        let css = platform_css();
        if cfg!(target_os = "macos") {
            assert!(css.contains("--conduit-chrome-side:flex-start"), "{}", css);
            // the traffic light sits 13px in, as it does in every macOS window
            assert!(css.contains("0 0 0 13px"), "{}", css);
        } else {
            assert!(css.contains("--conduit-chrome-side:flex-end"), "{}", css);
            assert!(css.contains("0 6px 0 0"), "{}", css);
        }
    }

    /// macOS's close button is a red disc, not a glyph. The whole point of
    /// the change is a thing this host cannot render, so what is checked is
    /// the sheet itself — and that it stays off every other platform, where
    /// a bare red circle would be nothing anyone recognises.
    #[test]
    fn the_macos_close_button_is_a_traffic_light() {
        let mac = traffic_light_css(true);
        assert!(mac.contains("#ff5f57"), "not the system red: {}", mac);
        assert!(mac.contains("border-radius: 50%"), "not a disc: {}", mac);
        assert!(mac.contains("width: 12px"), "not the system size: {}", mac);
        // the glyph is hidden until hover, which is the half that is easy
        // to lose and leaves a permanently crossed-out button behind
        assert!(mac.contains("opacity: 0;"), "glyph not hidden: {}", mac);
        assert!(mac.contains(":hover::before"), "glyph never revealed: {}", mac);

        assert_eq!(traffic_light_css(false), "");
    }

    /// Both sheets style `#__conduit-chrome button`, and CHROME is injected
    /// at the end of the body — so the traffic light only wins by coming
    /// after it. Reversed, the disc would silently be a plain ✕ again.
    #[test]
    fn the_traffic_light_follows_the_chrome_it_overrides() {
        let html = chrome_html(true);
        let chrome = html.find("__conduit-chrome-style").expect("chrome sheet");
        let light = html.find("__conduit-traffic-light").expect("platform sheet");
        assert!(chrome < light, "traffic light must come after the chrome");
    }

    /// The colors someone sampled off their own screen are theirs. An
    /// imported package is third-party code that is otherwise handed
    /// nothing but its own files, and this must not become the exception.
    #[test]
    fn only_built_in_packages_are_handed_the_color_history() {
        let history = || r##"["#ff8800"]"##.to_string();

        let builtin = colors_js("conduit.devtools", history);
        assert!(builtin.contains("__conduitColors"), "{}", builtin);
        assert!(builtin.contains("#ff8800"), "{}", builtin);

        assert_eq!(colors_js("some.imported.package", history), "");
        assert_eq!(colors_js("conduit.devtools.evil", history), "");
    }

    /// The variables have to reach the document or the bar falls back to
    /// its defaults, which would silently be the Windows placement.
    #[test]
    fn the_chrome_reads_the_platform_variables() {
        assert!(CHROME.contains("var(--conduit-chrome-side"), "bar side not variable");
        assert!(
            CHROME.contains("var(--conduit-chrome-button-margin"),
            "button margin not variable"
        );
    }

    /// The bar's height lives twice in CHROME: once as CSS, once as the
    /// padding the script reserves so the app's own content clears it. If
    /// they drift the content slides under the bar, which looks like a
    /// styling accident rather than a mismatch between two constants.
    #[test]
    fn the_chrome_bar_reserves_exactly_its_own_height() {
        let css = CHROME
            .split("#__conduit-chrome {")
            .nth(1)
            .and_then(|rest| rest.split("height:").nth(1))
            .and_then(|rest| rest.split("px").next())
            .map(|value| value.trim().to_string())
            .expect("chrome bar height in CSS");
        let script = CHROME
            .split("var BAR_HEIGHT =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .map(|value| value.trim().to_string())
            .expect("BAR_HEIGHT in the script");
        assert_eq!(css, script, "CSS bar height and BAR_HEIGHT disagree");
    }

    /// The transparent gutter is window too; without this only the bar and
    /// the app's own surface can move it.
    #[test]
    fn the_gutter_is_draggable() {
        assert!(CHROME.contains(r#"documentElement.setAttribute("data-tauri-drag-region", "")"#));
    }

    use super::*;

    #[test]
    fn accepts_plain_relative_paths() {
        assert_eq!(sanitize_rel_path("tool.html"), Some(PathBuf::from("tool.html")));
        assert_eq!(
            sanitize_rel_path("sub/dir/app.html"),
            Some(PathBuf::from("sub/dir/app.html"))
        );
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for bad in [
            "",
            "/etc/passwd",
            "../outside.html",
            "a/../../b.html",
            "a/./b.html",
            "a//b.html",
            "C:/windows/system32",
            "a\\b.html",
        ] {
            assert_eq!(sanitize_rel_path(bad), None, "should reject: {}", bad);
        }
    }

    fn theme(colors: &[(&str, &str)]) -> RenderedTheme {
        RenderedTheme {
            mode: "dark".into(),
            colors: colors
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// Guards the odd-looking transparent gradient on <html>: without a
    /// background-image there, the body background propagates to the
    /// canvas and the window's transparent gutter fills in.
    #[test]
    fn chrome_stops_background_propagation() {
        let html_rule = CHROME
            .split("html {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("chrome styles the root element");
        assert!(
            html_rule.contains("background-image:"),
            "root needs a background-image to stop propagation: {}",
            html_rule
        );
    }

    #[test]
    fn palette_becomes_conduit_variables() {
        let css = palette_css(Some(&theme(&[("surface", "#0b0a0d"), ("radius", "28px")])));
        assert!(css.contains("--conduit-surface:#0b0a0d;"), "{}", css);
        assert!(css.contains("--conduit-radius:28px;"), "{}", css);
        assert!(css.starts_with("<style"));
    }

    #[test]
    fn no_palette_before_the_launcher_reports() {
        assert_eq!(palette_css(None), "");
        assert_eq!(palette_css(Some(&theme(&[]))), "");
    }

    #[test]
    fn chrome_goes_inside_the_body() {
        let out = inject_chrome("<html><body><h1>hi</h1></BODY></html>", "", "");
        let chrome = out.find("__conduit-chrome").expect("chrome injected");
        let body_end = out.to_lowercase().rfind("</body>").unwrap();
        assert!(chrome < body_end, "chrome must precede the closing body tag");
        assert!(out.ends_with("</html>"));
    }

    /// An app reads window.__conduitStrings as it parses, so the strings
    /// have to be in the head — appended at the end of the body they would
    /// arrive after the app has already read an empty object.
    #[test]
    fn strings_land_before_the_app_script() {
        let out = inject_chrome(
            "<html><head><title>t</title></HEAD><body><script>read()</script></body></html>",
            "<style>p</style>",
            "<script>strings</script>",
        );
        let strings = out.find("<script>strings</script>").expect("strings injected");
        let palette = out.find("<style>p</style>").expect("palette injected");
        let app = out.find("<script>read()</script>").expect("app script kept");
        assert!(palette < strings, "palette first, then strings");
        assert!(strings < app, "strings must precede the app script");
        assert!(strings < out.to_lowercase().find("</head>").unwrap());
    }

    #[test]
    fn documents_without_a_head_still_get_strings_first() {
        let out = inject_chrome("<body><script>read()</script></body>", "", "<script>s</script>");
        assert!(out.find("<script>s</script>").unwrap() < out.find("<script>read()</script>").unwrap());
    }

    #[test]
    fn fragments_without_a_body_tag_still_get_chrome() {
        let out = inject_chrome("<h1>hi</h1>", "<style>x</style>", "<script>y</script>");
        assert!(out.contains("<style>x</style>"), "palette rides along");
        assert!(out.contains("<script>y</script>"), "strings ride along");
        assert!(out.contains("<h1>hi</h1>"), "the document survives");
        // head content leads, chrome trails
        assert!(out.find("<style>x</style>").unwrap() < out.find("<h1>hi</h1>").unwrap());
        assert!(out.find("<h1>hi</h1>").unwrap() < out.find("__conduit-chrome").unwrap());
    }

    #[test]
    fn only_html_files_are_injected() {
        assert!(is_html(&PathBuf::from("a/tool.html")));
        assert!(is_html(&PathBuf::from("TOOL.HTM")));
        for other in ["app.js", "style.css", "icon.png", "noext"] {
            assert!(!is_html(&PathBuf::from(other)), "should not inject: {}", other);
        }
    }

    #[test]
    fn case_insensitive_search_finds_the_last_match() {
        assert_eq!(rfind_ascii_ci("x</body>y</BODY>z", "</body>"), Some(9));
        assert_eq!(rfind_ascii_ci("日本語</BoDy>", "</body>"), Some(9));
        assert_eq!(rfind_ascii_ci("nothing", "</body>"), None);
    }

    #[test]
    fn workflow_id_rule_matches_manifest_validation() {
        assert!(valid_workflow_id("conduit.devtools"));
        assert!(valid_workflow_id("my-tools_2"));
        assert!(!valid_workflow_id(""));
        assert!(!valid_workflow_id("../evil"));
        assert!(!valid_workflow_id("a/b"));
    }
}
