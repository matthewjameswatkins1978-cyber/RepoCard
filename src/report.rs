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

    // Fully produce the new JSON in a sibling temp file first. `write`
    // returns only once all bytes are on disk (or errors with nothing
    // partially promised). The final path is then touched exactly once, by
    // rename. There is deliberately NO fallback to direct overwrite: on
    // Windows especially, a failed direct write could leave a half-written
    // final report next to a lost original.
    let tmp = destination.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, json.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(RepoCardError::Io(format!("write temp report: {e}")));
    }
    let bytes_written = json.len() as u64;
    if destination.exists() {
        // Windows rename cannot replace an existing file, so remove the old
        // report first. If removal fails the old report is untouched and the
        // temp file remains for inspection. If rename then fails, the worst
        // case is a missing final report plus an intact temp file -- never a
        // half-written final report.
        if let Err(e) = std::fs::remove_file(destination) {
            let _ = std::fs::remove_file(&tmp);
            return Err(RepoCardError::Io(format!(
                "replace previous report (old report left intact): {e}"
            )));
        }
    }
    if let Err(e) = std::fs::rename(&tmp, destination) {
        return Err(RepoCardError::Io(format!(
            "rename temp report into place (final report not half-written; temp may remain): {e}"
        )));
    }
    Ok(ReportReceipt {
        destination: destination.to_string_lossy().into_owned(),
        bytes_written,
        schema_version: SCHEMA_VERSION.to_string(),
    })
}
