use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::config::ConfigState;
use crate::plugins::workflows::{
    builtin, html_result_id, load_workflows, web_result_id, workflows_dir, WorkflowManifest,
};

/// Import a workflow package (zip) and install it under the workflows dir.
/// Returns the imported workflow's display name.
#[tauri::command]
pub async fn import_workflow(path: String, app: AppHandle) -> Result<String, String> {
    let zip_path = PathBuf::from(&path);
    tokio::task::spawn_blocking(move || import_zip(&zip_path, &app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// A workflow package as shown in the manage view: the manifest plus the
/// state the UI acts on, and per-app result ids ready to feed straight
/// into `execute_action` (built centrally so the id format lives in one
/// place).
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowListing {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub builtin: bool,
    pub disabled: bool,
    pub apps: Vec<AppListing>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppListing {
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub keyword: Option<String>,
    pub url: Option<String>,
    pub html: Option<String>,
    pub result_id: String,
}

/// List installed workflows for the manage view
#[tauri::command]
pub async fn list_workflows(
    app: AppHandle,
    config: State<'_, ConfigState>,
) -> Result<Vec<WorkflowListing>, String> {
    let dir = workflows_dir(&app).map_err(|e| e.to_string())?;
    let disabled = config.get().await.workflows.disabled;
    Ok(load_workflows(&dir)
        .into_iter()
        .map(|w| to_listing(w, &disabled))
        .collect())
}

fn to_listing(manifest: WorkflowManifest, disabled: &[String]) -> WorkflowListing {
    let apps = manifest
        .apps
        .iter()
        .map(|app| {
            let result_id = match (&app.url, &app.html) {
                (_, Some(html)) => html_result_id(&manifest.id, html),
                // {query} apps launched without a query open the base URL,
                // same as a keyword-less fuzzy match in search
                (Some(url), None) => web_result_id(&url.replace("{query}", "")),
                (None, None) => String::new(),
            };
            AppListing {
                name: crate::plugins::workflows::app_name(&manifest, app),
                icon: app.icon.clone(),
                description: crate::plugins::workflows::app_description(&manifest, app),
                keyword: app.keyword.clone(),
                url: app.url.clone(),
                html: app.html.clone(),
                result_id,
            }
        })
        .collect();
    WorkflowListing {
        builtin: builtin::is_builtin(&manifest.id),
        disabled: disabled.contains(&manifest.id),
        name: crate::plugins::workflows::package_name(&manifest),
        description: crate::plugins::workflows::package_description(&manifest),
        id: manifest.id,
        icon: manifest.icon,
        apps,
    }
}

/// Enable / disable a workflow package (persisted in config.json)
#[tauri::command]
pub async fn set_workflow_enabled(
    id: String,
    enabled: bool,
    config: State<'_, ConfigState>,
) -> Result<(), String> {
    let mut cfg = config.get().await;
    cfg.workflows.disabled.retain(|d| d != &id);
    if !enabled {
        cfg.workflows.disabled.push(id);
    }
    config.update(cfg).await.map_err(|e| e.to_string())
}

/// Delete an installed workflow package. Built-in packages are refused —
/// their files ship with the binary and would reappear on next start, so
/// disabling is the honest off switch for them.
#[tauri::command]
pub async fn delete_workflow(id: String, app: AppHandle) -> Result<(), String> {
    if builtin::is_builtin(&id) {
        return Err(crate::i18n::t("errors", "workflow_builtin_delete").into());
    }
    if !is_safe_workflow_id(&id) {
        return Err(crate::i18n::tf("errors", "workflow_bad_id", &[("id", &id)]));
    }
    let dir = workflows_dir(&app).map_err(|e| e.to_string())?.join(&id);
    if !dir.is_dir() {
        return Err(crate::i18n::tf("errors", "workflow_not_found", &[("id", &id)]));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Write an installed workflow out as a zip that `import_workflow` accepts.
///
/// The built-in Dev Tools is the reason this exists: it is the only package
/// most people will ever have, and being able to unzip a working one beats
/// any amount of prose about the format. Everything installed can be
/// exported, though — a package edited in place is worth getting back out.
#[tauri::command]
pub async fn export_workflow(id: String, dest: String, app: AppHandle) -> Result<String, String> {
    if !is_safe_workflow_id(&id) {
        return Err(crate::i18n::tf("errors", "workflow_bad_id", &[("id", &id)]));
    }
    let dir = workflows_dir(&app).map_err(|e| e.to_string())?.join(&id);
    if !dir.is_dir() {
        return Err(crate::i18n::tf("errors", "workflow_not_found", &[("id", &id)]));
    }
    let dest = PathBuf::from(dest);
    tokio::task::spawn_blocking(move || zip_dir(&dir, &dest).map(|_| dest.display().to_string()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| {
            crate::i18n::tf("errors", "zip_write", &[("error", &e.to_string())])
        })
}

/// Zip a directory with its contents at the archive root, which is the
/// layout `find_manifest` treats as the plain case (manifest.json at the
/// top). Nested directories are walked so an app with assets survives.
fn zip_dir(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    fn walk(
        zip: &mut zip::ZipWriter<std::fs::File>,
        options: zip::write::FileOptions<'_, ()>,
        base: &Path,
        dir: &Path,
    ) -> anyhow::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        // Stable order so exporting the same package twice gives the same
        // archive; read_dir order is whatever the filesystem feels like.
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let rel = path.strip_prefix(base)?.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                zip.add_directory(format!("{}/", rel), options)?;
                walk(zip, options, base, &path)?;
            } else {
                zip.start_file(rel, options)?;
                let mut source = std::fs::File::open(&path)?;
                std::io::copy(&mut source, zip)?;
            }
        }
        Ok(())
    }

    walk(&mut zip, options, src, src)?;
    zip.finish()?;
    Ok(())
}

/// The id becomes a path component, so it may not be able to escape the
/// workflows directory. Imports enforce the same rule.
fn is_safe_workflow_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn import_zip(zip_path: &Path, app: &AppHandle) -> anyhow::Result<String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| anyhow::anyhow!(crate::i18n::tf("errors", "zip_open", &[("error", &e.to_string())])))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| anyhow::anyhow!(crate::i18n::tf("errors", "zip_read", &[("error", &e.to_string())])))?;

    // Find manifest.json at the root, or one directory deep (GitHub-style zips)
    let (manifest_entry, prefix) = find_manifest(&mut archive)
        .ok_or_else(|| anyhow::anyhow!(crate::i18n::t("errors", "manifest_missing")))?;

    let manifest: WorkflowManifest = {
        let mut entry = archive.by_name(&manifest_entry)?;
        let mut contents = String::new();
        entry.read_to_string(&mut contents)?;
        serde_json::from_str(&contents)
            .map_err(|e| {
                anyhow::anyhow!(crate::i18n::tf(
                    "errors",
                    "manifest_invalid",
                    &[("error", &e.to_string())]
                ))
            })?
    };

    validate_manifest(&manifest)?;

    // Install: replace any existing workflow with the same id
    let dest = workflows_dir(app)?.join(&manifest.id);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(&dest)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        // enclosed_name() rejects path traversal (zip-slip)
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = name.strip_prefix(&prefix) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }

        let out_path = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }

    Ok(manifest.name)
}

/// Locate manifest.json in the archive.
/// Returns (entry name, path prefix to strip on extraction).
fn find_manifest(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<(String, PathBuf)> {
    let mut candidate: Option<(String, PathBuf)> = None;

    for name in archive.file_names() {
        if name == "manifest.json" {
            // Root-level manifest wins immediately
            return Some((name.to_string(), PathBuf::new()));
        }
        // One directory deep, e.g. "my-workflow-main/manifest.json"
        if let Some(dir) = name.strip_suffix("/manifest.json") {
            if !dir.contains('/') && candidate.is_none() {
                candidate = Some((name.to_string(), PathBuf::from(dir)));
            }
        }
    }
    candidate
}

pub fn validate_manifest(manifest: &WorkflowManifest) -> anyhow::Result<()> {
    if manifest.id.trim().is_empty() || manifest.name.trim().is_empty() {
        anyhow::bail!(crate::i18n::t("errors", "manifest_id_name"));
    }
    // id becomes a directory name: restrict to safe characters
    if !manifest
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        anyhow::bail!(crate::i18n::tf(
            "errors",
            "manifest_id_chars",
            &[("id", &manifest.id)]
        ));
    }
    if manifest.apps.is_empty() {
        anyhow::bail!(crate::i18n::t("errors", "manifest_no_apps"));
    }
    for app in &manifest.apps {
        if app.name.trim().is_empty() {
            anyhow::bail!(crate::i18n::t("errors", "manifest_app_name"));
        }
        match (&app.url, &app.html) {
            (Some(url), None) => {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    anyhow::bail!(crate::i18n::tf("errors", "manifest_app_url", &[("url", url)]));
                }
            }
            (None, Some(html)) => {
                if crate::plugins::workflows::tool_window::sanitize_rel_path(html).is_none() {
                    anyhow::bail!(crate::i18n::tf(
                        "errors",
                        "manifest_app_html_path",
                        &[("path", html)]
                    ));
                }
                if !html.to_ascii_lowercase().ends_with(".html")
                    && !html.to_ascii_lowercase().ends_with(".htm")
                {
                    anyhow::bail!(crate::i18n::tf(
                        "errors",
                        "manifest_app_html_ext",
                        &[("path", html)]
                    ));
                }
            }
            (Some(_), Some(_)) => {
                anyhow::bail!(crate::i18n::tf(
                    "errors",
                    "manifest_app_both",
                    &[("name", &app.name)]
                ))
            }
            (None, None) => {
                anyhow::bail!(crate::i18n::tf(
                    "errors",
                    "manifest_app_neither",
                    &[("name", &app.name)]
                ))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The point of the export is that the result can be imported again.
    /// Zipping and re-reading is the only way to know the layout matches
    /// what find_manifest expects — a nested or prefixed archive still
    /// looks fine on disk and only fails at import time.
    #[test]
    fn an_exported_package_reimports_with_the_same_files() {
        let tmp = std::env::temp_dir().join(format!("conduit-export-{}", std::process::id()));
        let src = tmp.join("pkg");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(src.join("assets")).unwrap();
        std::fs::write(
            src.join("manifest.json"),
            r#"{"id":"sample.pkg","name":"Sample","apps":[{"name":"A","html":"a.html"}]}"#,
        )
        .unwrap();
        std::fs::write(src.join("a.html"), "<h1>hi</h1>").unwrap();
        std::fs::write(src.join("assets").join("x.css"), "body{}").unwrap();

        let zip_path = tmp.join("out.zip");
        super::zip_dir(&src, &zip_path).expect("export");

        let archive = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(archive).unwrap();

        // find_manifest is what import runs first; if it cannot locate the
        // manifest the package is rejected however good the contents are.
        let (entry, prefix) = super::find_manifest(&mut archive).expect("manifest found");
        assert_eq!(entry, "manifest.json");
        assert_eq!(prefix, std::path::PathBuf::from(""));

        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .filter(|n| !n.ends_with('/'))
            .collect();
        names.sort();
        assert_eq!(names, vec!["a.html", "assets/x.css", "manifest.json"]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workflow_ids_that_could_escape_the_directory_are_refused() {
        assert!(super::is_safe_workflow_id("conduit.devtools"));
        assert!(!super::is_safe_workflow_id(""));
        assert!(!super::is_safe_workflow_id(".."));
        assert!(!super::is_safe_workflow_id("../evil"));
        assert!(!super::is_safe_workflow_id("a/b"));
    }

    use super::*;
    use super::validate_manifest;
    use crate::plugins::workflows::WorkflowManifest;

    /// The listing's result ids must parse in `execute` exactly like the
    /// ids search builds — html ids strip to (workflow_id, html).
    #[test]
    fn listing_result_ids_match_the_execute_format() {
        let manifest: WorkflowManifest = serde_json::from_str(
            r#"{"id":"t","name":"T","apps":[
                {"name":"web","url":"https://example.com/?q={query}"},
                {"name":"tool","html":"tool.html"}]}"#,
        )
        .unwrap();
        let listing = to_listing(manifest, &["other".into()]);
        assert!(!listing.builtin);
        assert!(!listing.disabled);
        assert_eq!(listing.apps[0].result_id, "conduit.workflows:https://example.com/?q=");
        assert_eq!(listing.apps[1].result_id, "conduit.workflows:html:t/tool.html");

        let rest = listing.apps[1]
            .result_id
            .strip_prefix("conduit.workflows:")
            .and_then(|r| r.strip_prefix("html:"))
            .and_then(|r| r.split_once('/'));
        assert_eq!(rest, Some(("t", "tool.html")));
    }

    #[test]
    fn builtin_and_disabled_flags_are_reported() {
        let manifest: WorkflowManifest = serde_json::from_str(
            r#"{"id":"conduit.devtools","name":"Dev Tools","apps":[{"name":"a","html":"a.html"}]}"#,
        )
        .unwrap();
        let listing = to_listing(manifest, &["conduit.devtools".into()]);
        assert!(listing.builtin);
        assert!(listing.disabled);
    }

    fn manifest(apps_json: &str) -> WorkflowManifest {
        serde_json::from_str(&format!(
            r#"{{"id":"t","name":"T","apps":{}}}"#,
            apps_json
        ))
        .unwrap()
    }

    #[test]
    fn accepts_web_and_html_apps() {
        assert!(validate_manifest(&manifest(
            r#"[{"name":"a","url":"https://example.com"},
                {"name":"b","html":"tool.html"},
                {"name":"c","html":"sub/app.htm"}]"#
        ))
        .is_ok());
    }

    #[test]
    fn rejects_ambiguous_and_empty_targets() {
        // both url and html
        assert!(validate_manifest(&manifest(
            r#"[{"name":"a","url":"https://x.com","html":"t.html"}]"#
        ))
        .is_err());
        // neither
        assert!(validate_manifest(&manifest(r#"[{"name":"a"}]"#)).is_err());
    }

    #[test]
    fn rejects_unsafe_html_paths() {
        for bad in ["../evil.html", "/abs.html", "a\\b.html", "script.js"] {
            let json = format!(r#"[{{"name":"a","html":"{}"}}]"#, bad.replace('\\', "\\\\"));
            assert!(
                validate_manifest(&manifest(&json)).is_err(),
                "should reject html path: {}",
                bad
            );
        }
    }
}
