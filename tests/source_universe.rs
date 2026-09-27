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

/// Filesystem walk, attention scan, test footprint and Tokei statistics must
/// operate over the same source universe: hidden files, ignored files and
/// `.repocard` output are not source.
#[test]
fn language_stats_follow_repocard_policy() {
    let t = TempDir::new().unwrap();
    git(t.path(), &["init", "-b", "main"]);
    git(t.path(), &["config", "user.name", "Test"]);
    git(t.path(), &["config", "user.email", "test@example.com"]);

    // Visible source: exactly 3 Rust code lines.
    fs::create_dir_all(t.path().join("src")).unwrap();
    fs::write(
        t.path().join("src/vis.rs"),
        "fn a() {}\nfn b() {}\nfn c() {}\n",
    )
    .unwrap();
    // Hidden source file: must be excluded everywhere.
    fs::write(t.path().join(".hidden.rs"), "fn h() {}\n".repeat(50)).unwrap();
    // Ignored source file (gitignored inside a real repo).
    fs::write(t.path().join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(t.path().join("ignored.rs"), "fn i() {}\n".repeat(50)).unwrap();
    // Report output: 200 lines of JSON that must never count as source.
    fs::create_dir_all(t.path().join(".repocard")).unwrap();
    let mut report_json = String::from("{\n");
    for i in 0..200 {
        report_json.push_str(&format!("  \"key{i}\": {i},\n"));
    }
    report_json.push_str("  \"done\": true\n}\n");
    fs::write(t.path().join(".repocard/report.json"), report_json).unwrap();

    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "fixture"]);

    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    let names: Vec<&str> = s
        .languages
        .languages
        .iter()
        .map(|l| l.language.as_str())
        .collect();
    // report.json must not leak JSON statistics into the snapshot.
    assert!(
        !names.contains(&"JSON"),
        "repocard output counted as source: {names:?}"
    );
    let rust = s
        .languages
        .languages
        .iter()
        .find(|l| l.language == "Rust")
        .expect("Rust stats present");
    assert_eq!(
        rust.code, 3,
        "only src/vis.rs may contribute Rust code: {rust:?}"
    );
    // Walk universe agrees: hidden/ignored/.repocard files are not scanned.
    assert!(s.filesystem.file_count < 8, "{}", s.filesystem.file_count);
    let walked: Vec<&str> = s
        .filesystem
        .largest_files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    for banned in [".hidden.rs", "ignored.rs", ".repocard/report.json"] {
        assert!(
            !walked.contains(&banned),
            "{banned} leaked into walk: {walked:?}"
        );
    }
}
