use crate::model::{RepoCardError, RepoSnapshot, ReportPlan, ReportReceipt, SCHEMA_VERSION};
use std::path::{Path, PathBuf};

pub fn default_destination(root: &Path) -> PathBuf {
    root.join(".repocard").join("report.json")
}

pub fn plan_report(root: &Path, destination: &Path) -> ReportPlan {
    let parent = destination.parent().map(|p| p.to_path_buf());
    let will_create_directory = parent.map(|p| !p.exists()).unwrap_or(false);
    let will_create_file = !destination.exists();
    let will_overwrite = destination.exists();
    let _ = root;
    ReportPlan {
        destination: destination.to_string_lossy().into_owned(),
        will_create_directory,
        will_create_file,
        will_overwrite,
        schema_version: SCHEMA_VERSION.to_string(),
    }
}

pub fn write_report(
    snapshot: &RepoSnapshot,
    destination: &Path,
    force: bool,
) -> Result<ReportReceipt, RepoCardError> {
    if destination.exists() && !force {
        return Err(RepoCardError::Exists(
            destination.to_string_lossy().into_owned(),
        ));
    }
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let json = serde_json::to_string_pretty(snapshot)
        .map_err(|e| RepoCardError::Scan(format!("serialize snapshot: {e}")))?;

    // Atomic-ish temp + rename.
    let tmp = destination.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())?;
    let bytes_written = json.len() as u64;
    match std::fs::rename(&tmp, destination) {
        Ok(()) => {}
        Err(_) => {
            // Fallback: direct write (cross-device), then remove tmp.
            std::fs::write(destination, json.as_bytes())?;
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Ok(ReportReceipt {
        destination: destination.to_string_lossy().into_owned(),
        bytes_written,
        schema_version: SCHEMA_VERSION.to_string(),
    })
}
