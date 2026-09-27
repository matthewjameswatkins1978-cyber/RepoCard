use repocard::{scan, ScanOptions};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &std::path::Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(st.success(), "git {args:?} failed");
}

#[test]
fn history_hot_files_bounded_and_deterministic() {
    let t = TempDir::new().unwrap();
    git(t.path(), &["init", "-b", "main"]);
    git(t.path(), &["config", "user.name", "Test"]);
    git(t.path(), &["config", "user.email", "test@example.com"]);
    fs::write(t.path().join("hot.txt"), "v0").unwrap();
    fs::write(t.path().join("cold.txt"), "v0").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "init"]);
    for i in 1..=4 {
        fs::write(t.path().join("hot.txt"), format!("v{i}")).unwrap();
        git(t.path(), &["commit", "-am", &format!("hot {i}")]);
    }
    fs::write(t.path().join("cold.txt"), "v1").unwrap();
    git(t.path(), &["commit", "-am", "cold 1"]);

    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    assert_eq!(s.history.commit_window, 200);
    assert!(s.history.commits_sampled >= 6);
    assert!(!s.history.hot_files.is_empty());
    assert_eq!(s.history.hot_files[0].path, "hot.txt");
    assert!(s.history.hot_files.len() <= 10);
}
