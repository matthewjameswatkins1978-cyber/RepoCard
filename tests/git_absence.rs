use repocard::{model::WarningSink, scan::scan_with_runner, scan::GitRunner, ScanOptions};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn git_absence_still_produces_filesystem_snapshot() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("a.txt"), "hello").unwrap();
    fs::write(t.path().join("b.py"), "print('hi')\n").unwrap();

    let runner = GitRunner {
        program: "repocard-nonexistent-git-binary-xyz".to_string(),
        timeout: Duration::from_secs(5),
    };
    let options = ScanOptions::default();
    let root = std::fs::canonicalize(t.path()).unwrap();
    let root_str = root.to_string_lossy().into_owned();
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap();
    let mut warnings = WarningSink::new(options.warnings_limit);
    let mut events = Vec::new();
    let snap = scan_with_runner(
        &root,
        &name,
        &root_str,
        &options,
        &runner,
        &mut |e| events.push(e),
        &mut warnings,
    )
    .expect("scan must survive missing git");
    assert!(snap.git.is_none());
    assert!(snap.filesystem.file_count >= 2);
    assert!(snap.warnings.iter().any(|w| w.category == "git"));
}

#[test]
fn non_git_directory_reports_warning_and_continues() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("main.rs"), "fn main() {}").unwrap();
    let snap = repocard::scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    // May or may not be inside a parent git repo depending on machine; if the
    // temp dir is not in a repo, git must be None with a warning.
    if snap.git.is_none() {
        assert!(snap.warnings.iter().any(|w| w.category == "git"));
        assert!(snap.filesystem.file_count >= 1);
    }
}
