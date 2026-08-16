//! Built-in workflow packages, embedded in the binary and written into the
//! workflows directory at startup. They then behave exactly like imported
//! packages (searchable, listed in the manage view, servable over the
//! custom protocol).

use tauri::AppHandle;

use super::{workflows_dir, WorkflowManifest};

/// The Dev Tools package: offline converter apps in their own windows
const DEVTOOLS: &[(&str, &str)] = &[
    ("manifest.json", include_str!("assets/devtools/manifest.json")),
    ("base64.html", include_str!("assets/devtools/base64.html")),
    ("json.html", include_str!("assets/devtools/json.html")),
    ("unixtime.html", include_str!("assets/devtools/unixtime.html")),
    ("color.html", include_str!("assets/devtools/color.html")),
    ("capture.html", include_str!("assets/devtools/capture.html")),
    ("theme.html", include_str!("assets/devtools/theme.html")),
];

/// Built-in package ids ship with the binary: deleting their directory
/// only means they come back on next start, so the manage view offers
/// disabling instead of deletion for them.
pub fn is_builtin(id: &str) -> bool {
    id == "conduit.devtools"
}

/// Locale key for a bundled app file: "base64.html" -> "base64", which
/// the catalog carries as `tools.base64_title` / `tools.base64_desc`.
/// Built-in apps follow the UI language; imported packages keep whatever
/// their own manifest says.
pub fn locale_key(html: &str) -> Option<&str> {
    html.strip_suffix(".html")
}

/// Install / refresh built-in packages. An installed copy is left alone
/// unless the embedded manifest carries a different `version`, so a user
/// who deletes the package directory gets it back on next start, while
/// local edits survive until the app ships a newer version.
pub fn install(app: &AppHandle) {
    let Ok(dir) = workflows_dir(app) else {
        return;
    };

    let embedded: WorkflowManifest = serde_json::from_str(DEVTOOLS[0].1)
        .expect("embedded devtools manifest must parse");

    let dest = dir.join(&embedded.id);
    let installed_version = std::fs::read_to_string(dest.join("manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<WorkflowManifest>(&s).ok())
        .and_then(|m| m.version);

    if dest.exists() && installed_version == embedded.version {
        return;
    }

    if let Err(e) = write_package(&dest, DEVTOOLS) {
        eprintln!("failed to install built-in workflow: {}", e);
    }
}

fn write_package(dest: &std::path::Path, files: &[(&str, &str)]) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for (name, contents) in files {
        std::fs::write(dest.join(name), contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;

    /// The queries a user will actually type must fuzzy-match the app
    /// names. "json" / "b64" reach these via the registry's empty-keyword
    /// fallback, so they must hit here too.
    #[test]
    fn devtools_apps_are_reachable_by_obvious_queries() {
        let manifest: WorkflowManifest = serde_json::from_str(DEVTOOLS[0].1).unwrap();
        let matcher = SkimMatcherV2::default();
        for (query, html) in [
            ("base64", "base64.html"),
            ("b64", "base64.html"),
            ("json", "json.html"),
            ("整形", "json.html"),
            ("unix", "unixtime.html"),
            ("time", "unixtime.html"),
            ("録画", "capture.html"),
            ("スクリーンショット", "capture.html"),
            ("カラーシステム", "theme.html"),
        ] {
            let hit = manifest.apps.iter().any(|app| {
                app.html.as_deref() == Some(html)
                    && matcher.fuzzy_match(&app.name, query).is_some()
            });
            assert!(hit, "query {:?} should match app {}", query, html);
        }
    }

    /// The embedded package must always satisfy the same validation that
    /// imported zips go through — otherwise startup installs a package the
    /// importer would have rejected.
    #[test]
    fn embedded_devtools_manifest_is_valid() {
        let manifest: WorkflowManifest = serde_json::from_str(DEVTOOLS[0].1).unwrap();
        crate::commands::workflow::validate_manifest(&manifest).unwrap();
        assert_eq!(manifest.id, "conduit.devtools");
        assert!(manifest.version.is_some());

        // every html app in the manifest ships in the package
        let shipped: Vec<&str> = DEVTOOLS.iter().map(|(n, _)| *n).collect();
        for app in &manifest.apps {
            let html = app.html.as_deref().expect("devtools apps are html apps");
            assert!(shipped.contains(&html), "missing from package: {}", html);
        }
    }
}
