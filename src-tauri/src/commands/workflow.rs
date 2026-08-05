use std::io::Read;
use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::plugins::workflows::{load_workflows, workflows_dir, WorkflowManifest};

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

/// List installed workflows (for future settings UI / status display)
#[tauri::command]
pub async fn list_workflows(app: AppHandle) -> Result<Vec<WorkflowManifest>, String> {
    let dir = workflows_dir(&app).map_err(|e| e.to_string())?;
    Ok(load_workflows(&dir))
}

fn import_zip(zip_path: &Path, app: &AppHandle) -> anyhow::Result<String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("zip を開けません: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| anyhow::anyhow!("zip として読み込めません: {}", e))?;

    // Find manifest.json at the root, or one directory deep (GitHub-style zips)
    let (manifest_entry, prefix) = find_manifest(&mut archive)
        .ok_or_else(|| anyhow::anyhow!("manifest.json が zip 内に見つかりません"))?;

    let manifest: WorkflowManifest = {
        let mut entry = archive.by_name(&manifest_entry)?;
        let mut contents = String::new();
        entry.read_to_string(&mut contents)?;
        serde_json::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("manifest.json の形式が不正です: {}", e))?
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

fn validate_manifest(manifest: &WorkflowManifest) -> anyhow::Result<()> {
    if manifest.id.trim().is_empty() || manifest.name.trim().is_empty() {
        anyhow::bail!("manifest の id / name は必須です");
    }
    // id becomes a directory name: restrict to safe characters
    if !manifest
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        anyhow::bail!("id には英数字と - _ . のみ使用できます: {}", manifest.id);
    }
    if manifest.apps.is_empty() {
        anyhow::bail!("apps が空です");
    }
    for app in &manifest.apps {
        if app.name.trim().is_empty() {
            anyhow::bail!("apps 内に name が空のエントリがあります");
        }
        if !app.url.starts_with("http://") && !app.url.starts_with("https://") {
            anyhow::bail!("URL は http(s):// で始まる必要があります: {}", app.url);
        }
    }
    Ok(())
}
