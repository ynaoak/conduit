//! Handing a search result to a tool window.
//!
//! The launcher's plugins produce values — a line you copied, a color you
//! picked, a converted string — and the tool windows consume them. What
//! was missing was the arrow between the two: a result had no way to
//! become another tool's input, so every tool started from an empty box
//! and a paste.
//!
//! A destination declares what it accepts (`accepts` in the manifest); a
//! result offers what it is (`ConduitPlugin::payload`). Everything here is
//! the matching between those two, plus the handover itself.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::plugin_system::types::{Action, Payload, SearchResult};
use crate::plugins::workflows::{load_workflows, package_name, tool_window, workflows_dir};

/// Action id prefix. The rest is `<workflow id>/<html>`, which is exactly
/// what `tool_window::open` takes.
pub const PREFIX: &str = "send:";

/// A tool window that accepts a kind of payload.
pub struct Target {
    pub workflow_id: String,
    pub html: String,
    /// What to call it in the action list
    pub label: String,
}

impl Target {
    pub fn action_id(&self) -> String {
        format!("{}{}/{}", PREFIX, self.workflow_id, self.html)
    }
}

/// Split an action id back into the package and the file it names.
pub fn parse_action(action_id: &str) -> Option<(&str, &str)> {
    action_id.strip_prefix(PREFIX)?.split_once('/')
}

/// Every installed app that accepts `kind`, in manifest order.
///
/// Disabled packages are excluded by the same rule the rest of the app
/// uses: a package the user switched off should not reappear as a
/// destination.
pub fn targets(app: &AppHandle, kind: &str) -> Vec<Target> {
    let Ok(dir) = workflows_dir(app) else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for manifest in load_workflows(&dir) {
        let package = package_name(&manifest);
        for definition in &manifest.apps {
            let Some(html) = definition.html.as_deref() else {
                continue;
            };
            if !definition.accepts.iter().any(|a| a == kind) {
                continue;
            }
            targets.push(Target {
                workflow_id: manifest.id.clone(),
                html: html.to_string(),
                label: app_label(&manifest, definition, &package),
            });
        }
    }
    targets
}

/// Built-in app names follow the UI language; imported ones show whatever
/// their manifest says, qualified by the package so two packages with a
/// "Format" app stay apart.
fn app_label(
    manifest: &crate::plugins::workflows::WorkflowManifest,
    definition: &crate::plugins::workflows::WebAppDef,
    package: &str,
) -> String {
    let name = crate::plugins::workflows::app_name(manifest, definition);
    if crate::plugins::workflows::builtin::is_builtin(&manifest.id) {
        name
    } else {
        format!("{package} — {name}")
    }
}

/// Append a "send to …" action for every destination that takes this kind.
pub fn decorate(app: &AppHandle, result: &mut SearchResult, payload: &Payload) {
    for target in targets(app, &payload.kind) {
        result.actions.push(Action {
            id: target.action_id(),
            title: crate::i18n::tf("app", "send_to", &[("name", &target.label)]),
            shortcut: None,
        });
    }
}

/// Payloads waiting for the window that will receive them, keyed by the
/// window's label.
///
/// The tool window has no IPC to ask with, so the value is injected into
/// the document when the protocol serves it — which means it has to be
/// parked somewhere between "the user chose a destination" and "that
/// window asks for its HTML".
#[derive(Default)]
pub struct PendingInput(Mutex<HashMap<String, Payload>>);

impl PendingInput {
    pub fn park(&self, label: String, payload: Payload) {
        if let Ok(mut pending) = self.0.lock() {
            pending.insert(label, payload);
        }
    }

    /// Whether this window has something waiting, without consuming it.
    pub fn has(&self, label: &str) -> bool {
        self.0.lock().map(|p| p.contains_key(label)).unwrap_or(false)
    }

    /// Taken, not read: a reload much later should not resurrect an input
    /// the user handed over once.
    pub fn take(&self, label: &str) -> Option<Payload> {
        self.0.lock().ok()?.remove(label)
    }

    /// The script tag for a served document, or nothing when this window
    /// is not expecting anything.
    pub fn script_for(&self, label: &str) -> String {
        match self.take(label) {
            Some(payload) => format!(
                "<script id=\"__conduit-input\">window.__conduitInput={};</script>",
                serde_json::to_string(&payload).unwrap_or_else(|_| "null".into())
            ),
            None => String::new(),
        }
    }
}

/// Hand a result to a tool window and bring it up.
#[tauri::command]
pub async fn send_to_tool(
    app: AppHandle,
    plugin_id: String,
    result_id: String,
    action_id: String,
    registry: tauri::State<'_, crate::plugin_system::registry::PluginRegistry>,
) -> Result<(), String> {
    let Some((workflow_id, html)) = parse_action(&action_id) else {
        return Err(format!("not a send action: {action_id}"));
    };
    let payload = registry
        .payload(&plugin_id, &result_id)
        .await
        .ok_or_else(|| crate::i18n::t("errors", "send_no_payload").to_string())?;

    // The destination has to have said it takes this, even though the
    // action came from a list we built: the list was built for the
    // results as they were, and the world may have moved since.
    let accepted = targets(&app, &payload.kind)
        .iter()
        .any(|t| t.workflow_id == workflow_id && t.html == html);
    if !accepted {
        return Err(crate::i18n::t("errors", "send_not_accepted").to_string());
    }

    app.state::<PendingInput>()
        .park(tool_window::window_label(workflow_id, html), payload);

    tool_window::open(&app, workflow_id, html)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_id_round_trips_through_its_parts() {
        let target = Target {
            workflow_id: "conduit.devtools".into(),
            html: "count.html".into(),
            label: "文字数カウンター".into(),
        };
        assert_eq!(target.action_id(), "send:conduit.devtools/count.html");
        assert_eq!(
            parse_action(&target.action_id()),
            Some(("conduit.devtools", "count.html"))
        );
    }

    /// Anything that is not a send action has to fall through to the
    /// plugin that owns the result, untouched.
    #[test]
    fn other_actions_are_not_mistaken_for_sends() {
        for id in ["", "hex", "delete", "sending", "send", "send:no-slash"] {
            assert_eq!(parse_action(id), None, "{id}");
        }
    }

    /// An input is consumed once. A window reopened later starts empty
    /// rather than replaying something the user sent an hour ago.
    #[test]
    fn a_parked_input_is_handed_over_only_once() {
        let pending = PendingInput::default();
        pending.park("tool-1".into(), Payload::text("hello"));

        let script = pending.script_for("tool-1");
        assert!(script.contains("__conduitInput"), "{script}");
        assert!(script.contains("hello"), "{script}");

        assert_eq!(pending.script_for("tool-1"), "");
    }

    /// The built-in package is the reference for what `accepts` means, so
    /// its manifest is checked here: a tool that quietly stops accepting
    /// text disappears from every "send to" list without an error.
    #[test]
    fn the_built_in_apps_accept_what_they_can_use() {
        let manifest: crate::plugins::workflows::WorkflowManifest = serde_json::from_str(
            include_str!("plugins/workflows/assets/devtools/manifest.json"),
        )
        .expect("the built-in manifest parses");

        let accepts = |html: &str| -> Vec<String> {
            manifest
                .apps
                .iter()
                .find(|a| a.html.as_deref() == Some(html))
                .unwrap_or_else(|| panic!("{html} is in the manifest"))
                .accepts
                .clone()
        };

        for html in ["count.html", "base64.html", "json.html", "unixtime.html"] {
            assert_eq!(accepts(html), vec!["text".to_string()], "{html}");
        }
        for html in ["color.html", "theme.html"] {
            assert_eq!(accepts(html), vec!["color".to_string()], "{html}");
        }
        assert_eq!(accepts("image.html"), vec!["image".to_string()]);
        // Nothing sensible arrives at a key generator or a screenshot
        // gallery, and offering them would be noise in every action list.
        for html in ["password.html", "sshkey.html", "capture.html"] {
            assert!(accepts(html).is_empty(), "{html}");
        }
    }

    /// A window nobody sent anything to gets no script at all — not an
    /// empty object it would have to tell apart from a real payload.
    #[test]
    fn a_window_with_nothing_waiting_gets_nothing() {
        assert_eq!(PendingInput::default().script_for("tool-2"), "");
    }

    /// The value is JSON, and a payload full of quotes and newlines has to
    /// survive being written into a script tag.
    #[test]
    fn an_awkward_value_survives_the_handover() {
        let pending = PendingInput::default();
        pending.park("tool-3".into(), Payload::text("a\"b\n</script>"));
        let script = pending.script_for("tool-3");
        assert!(script.contains("a\\\"b\\n"), "{script}");
        // A closing tag inside the value would end the script early
        assert!(!script.contains("</script>\";"), "{script}");
    }
}
