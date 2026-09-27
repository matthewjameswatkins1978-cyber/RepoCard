use crate::model::{RepoCardError, RepoSnapshot, ReportPlan, ReportReceipt, SCHEMA_VERSION};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static BACKUP_ID: AtomicU64 = AtomicU64::new(0);

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

    // Fully produce the new JSON in a sibling temp file first.
    let tmp = destination.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, json.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(RepoCardError::Io(format!("write temp report: {e}")));
    }
    let bytes_written = json.len() as u64;
    if let Err(e) = replace_report(&tmp, destination, |from, to| std::fs::rename(from, to)) {
        return Err(RepoCardError::Io(format!(
            "replace report (previous report is preserved at its original or recovery path): {e}"
        )));
    }
    Ok(ReportReceipt {
        destination: destination.to_string_lossy().into_owned(),
        bytes_written,
        schema_version: SCHEMA_VERSION.to_string(),
    })
}

fn replace_report<F>(tmp: &Path, destination: &Path, mut rename: F) -> std::io::Result<()>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    if !destination.exists() {
        return rename(tmp, destination);
    }
    if !std::fs::symlink_metadata(destination)?
        .file_type()
        .is_file()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "report destination is not a regular file",
        ));
    }

    // Windows rename cannot replace an existing file. Move the old report to
    // a unique sibling first, then install the fully written temp. If install
    // fails, restore the old report; if restoration also fails, leave it at
    // the explicit backup path instead of deleting the only good copy.
    let id = BACKUP_ID.fetch_add(1, Ordering::Relaxed);
    let backup = destination.with_extension(format!("json.{}.{}.bak", std::process::id(), id));
    rename(destination, &backup)?;
    if let Err(install_error) = rename(tmp, destination) {
        return match rename(&backup, destination) {
            Ok(()) => Err(install_error),
            Err(restore_error) => Err(std::io::Error::new(
                restore_error.kind(),
                format!(
                    "install failed ({install_error}); restore failed ({restore_error}); old report remains at {}",
                    backup.display()
                ),
            )),
        };
    }
    // New report is now installed. Backup cleanup is best-effort; a leftover
    // backup is recoverable and safer than turning a successful write into loss.
    let _ = std::fs::remove_file(backup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::replace_report;
    use std::cell::Cell;
    use std::fs;
    use std::io;
    use tempfile::TempDir;

    #[test]
    fn failed_install_restores_previous_report() {
        let dir = TempDir::new().unwrap();
        let old = dir.path().join("report.json");
        let tmp = dir.path().join("report.json.tmp");
        fs::write(&old, b"old report").unwrap();
        fs::write(&tmp, b"new report").unwrap();
        let calls = Cell::new(0);

        let result = replace_report(&tmp, &old, |from, to| {
            let n = calls.get() + 1;
            calls.set(n);
            if n == 2 {
                return Err(io::Error::other("simulated install failure"));
            }
            fs::rename(from, to)
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&old).unwrap(), b"old report");
        assert_eq!(fs::read(&tmp).unwrap(), b"new report");
        assert_eq!(calls.get(), 3, "old report should be restored");
    }
}
