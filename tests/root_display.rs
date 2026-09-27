use repocard::scan::resolve_scan_root;
use std::path::PathBuf;

/// The `\\?\` extended-length prefix is an implementation detail: canonical
/// paths keep it internally, but the semantic root must never carry it.
#[test]
fn display_root_strips_extended_prefix() {
    assert_eq!(
        repocard::display_root(&PathBuf::from(r"\\?\D:\RepoCard\RepoCardv1")),
        r"D:\RepoCard\RepoCardv1"
    );
    assert_eq!(
        repocard::display_root(&PathBuf::from(r"\\?\UNC\server\share")),
        r"\\server\share"
    );
    assert_eq!(
        repocard::display_root(&PathBuf::from(r"D:\plain\path")),
        r"D:\plain\path"
    );
    assert_eq!(
        repocard::display_root(&PathBuf::from("/unix/style")),
        "/unix/style"
    );
}

#[test]
fn scanned_root_has_no_extended_prefix() {
    let t = tempfile::TempDir::new().unwrap();
    std::fs::write(t.path().join("a.txt"), "x").unwrap();
    let s = repocard::scan(t.path(), &repocard::ScanOptions::default(), |_| {}).unwrap();
    assert!(
        !s.repository.root.contains(r"\\?\"),
        "root leaked extended prefix: {}",
        s.repository.root
    );
    #[cfg(windows)]
    assert!(
        s.repository.root.contains(':'),
        "expected drive-letter root: {}",
        s.repository.root
    );
}

#[test]
fn resolve_scan_root_rejects_missing_and_files() {
    let t = tempfile::TempDir::new().unwrap();
    assert!(resolve_scan_root(&t.path().join("nope-missing")).is_err());
    let f = t.path().join("file.txt");
    std::fs::write(&f, b"x").unwrap();
    assert!(resolve_scan_root(&f).is_err());
    assert!(resolve_scan_root(t.path()).is_ok());
}

#[test]
fn dry_run_missing_root_fails_without_plan() {
    let exe = env!("CARGO_BIN_EXE_repocard");
    let missing = tempfile::TempDir::new()
        .unwrap()
        .path()
        .join("definitely-missing-dir");
    let out = std::process::Command::new(exe)
        .args(["write"])
        .arg(&missing)
        .arg("--dry-run")
        .output()
        .expect("run repocard binary");
    assert!(!out.status.success(), "dry-run on missing root must fail");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("will_create_directory"),
        "no plausible plan may be printed: {stdout}"
    );
}
