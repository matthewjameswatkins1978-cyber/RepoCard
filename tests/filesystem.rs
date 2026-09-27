use repocard::{scan, ScanOptions};
use std::fs;
use tempfile::TempDir;

fn scan_dir(dir: &std::path::Path) -> repocard::RepoSnapshot {
    scan(dir, &ScanOptions::default(), |_| {}).expect("scan ok")
}

#[test]
fn empty_directory_scans() {
    let t = TempDir::new().unwrap();
    let s = scan_dir(t.path());
    assert_eq!(s.filesystem.file_count, 0);
    assert!(!s.repository.is_git_repository);
    assert!(s.git.is_none());
    assert_eq!(s.schema_version, "repocard.v0.1");
    assert!(!s.partial);
}

#[test]
fn nested_files_counted() {
    let t = TempDir::new().unwrap();
    fs::create_dir_all(t.path().join("a/b")).unwrap();
    fs::write(t.path().join("a/b/c.txt"), "hello").unwrap();
    fs::write(t.path().join("top.rs"), "fn main() {}").unwrap();
    let s = scan_dir(t.path());
    assert!(s.filesystem.file_count >= 2);
    assert!(s.filesystem.scanned_bytes >= 5);
}

#[test]
fn gitignore_excludes_files() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(t.path().join("ignored.txt"), "x").unwrap();
    fs::write(t.path().join("kept.txt"), "y").unwrap();
    let s = scan_dir(t.path());
    let paths: Vec<&str> = s
        .filesystem
        .largest_files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert!(
        !paths.contains(&"ignored.txt"),
        "gitignored file leaked: {paths:?}"
    );
}

#[test]
fn paths_with_spaces_and_unicode() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("with space.txt"), "a").unwrap();
    fs::write(t.path().join("ünicode_ß.rs"), "fn f() {}").unwrap();
    let s = scan_dir(t.path());
    let json = serde_json::to_value(&s).unwrap();
    let str_repr = json.to_string();
    assert!(str_repr.contains("with space.txt"));
}

#[test]
fn large_file_ordering_and_threshold() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("small.txt"), vec![b'a'; 10]).unwrap();
    fs::write(t.path().join("big.txt"), vec![b'b'; 100]).unwrap();
    fs::write(t.path().join("mid.txt"), vec![b'c'; 50]).unwrap();
    let opts = ScanOptions {
        largest_files_limit: 2,
        large_file_threshold_bytes: 60,
        ..ScanOptions::default()
    };
    let s = scan(t.path(), &opts, |_| {}).unwrap();
    assert_eq!(s.filesystem.largest_files.len(), 2);
    assert_eq!(s.filesystem.largest_files[0].path, "big.txt");
    assert_eq!(s.filesystem.largest_files[1].path, "mid.txt");
    assert_eq!(s.filesystem.large_files_over_threshold, 1);
}

#[test]
fn deterministic_ties_sorted_by_path() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("b.txt"), vec![b'a'; 20]).unwrap();
    fs::write(t.path().join("a.txt"), vec![b'a'; 20]).unwrap();
    let s = scan_dir(t.path());
    let top: Vec<&str> = s
        .filesystem
        .largest_files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    let pa = top.iter().position(|p| *p == "a.txt").unwrap();
    let pb = top.iter().position(|p| *p == "b.txt").unwrap();
    assert!(pa < pb, "ties must be path-ordered: {top:?}");
}

#[test]
fn hidden_metadata_detected_independently_of_walk() {
    let t = TempDir::new().unwrap();
    fs::create_dir_all(t.path().join(".github/workflows")).unwrap();
    fs::write(t.path().join(".github/workflows/ci.yml"), "on: push").unwrap();
    fs::write(t.path().join("src.txt"), "x").unwrap();
    let s = scan_dir(t.path());
    assert!(s.project.ci.contains(&"github_actions".to_string()));
}
