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
    let root_str = repocard::display_root(&root);
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
    assert!(!snap.repository.is_git_repository);
    assert!(snap.filesystem.file_count >= 2);
    // Missing executable is a material gap: warning + partial.
    assert!(snap.warnings.iter().any(|w| w.category == "git"));
    assert!(snap.partial);
}

#[test]
fn ordinary_non_git_directory_is_complete_not_partial() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("main.rs"), "fn main() {}").unwrap();
    let snap = repocard::scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    if snap.git.is_none() && !snap.repository.is_git_repository {
        // Supported target: no git warning, not partial merely for being non-Git.
        assert!(
            !snap.warnings.iter().any(|w| w.category == "git"),
            "ordinary dir must not warn about git: {:?}",
            snap.warnings
        );
        assert!(!snap.partial, "ordinary dir must not be partial");
        assert!(snap.filesystem.file_count >= 1);
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(st.success(), "git {args:?} failed");
}

#[test]
fn status_failure_inside_repo_keeps_git_identity_and_marks_partial() {
    // Corrupt the index: rev-parse --show-toplevel still succeeds (identity
    // is Git) but status cannot be read.
    let t = TempDir::new().unwrap();
    git(t.path(), &["init", "-b", "main"]);
    git(t.path(), &["config", "user.name", "Test"]);
    git(t.path(), &["config", "user.email", "test@example.com"]);
    fs::write(t.path().join("a.txt"), "hi").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "init"]);
    fs::write(t.path().join(".git").join("index"), b"corrupt!!").unwrap();
    let s = repocard::scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    assert!(s.repository.is_git_repository, "identity must remain Git");
    assert!(s.git.is_none(), "unreadable status yields no snapshot");
    assert!(
        s.warnings.iter().any(|w| w.category == "git"),
        "{:?}",
        s.warnings
    );
    assert!(s.partial);
    // Filesystem facts still survive.
    assert!(s.filesystem.file_count >= 1);
}

#[test]
fn missing_executable_classifies_as_unavailable() {
    // Contract at the availability level: a missing executable is
    // Unavailable (warning + partial downstream), never silently NotRepo.
    let t = TempDir::new().unwrap();
    let root = std::fs::canonicalize(t.path()).unwrap();
    let missing = GitRunner {
        program: "repocard-nonexistent-git-binary-xyz".to_string(),
        timeout: Duration::from_secs(5),
    };
    match repocard::scan::git::availability(&root, &missing) {
        repocard::GitAvailability::Unavailable(_) => {}
        other => panic!(
            "missing git must be Unavailable, got {}",
            availability_name(&other)
        ),
    }
}

fn availability_name(a: &repocard::GitAvailability) -> &'static str {
    match a {
        repocard::GitAvailability::Repo(_) => "Repo",
        repocard::GitAvailability::NotRepo => "NotRepo",
        repocard::GitAvailability::Unavailable(_) => "Unavailable",
    }
}
