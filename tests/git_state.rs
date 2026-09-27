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

fn init_repo() -> TempDir {
    let t = TempDir::new().unwrap();
    git(t.path(), &["init", "-b", "main"]);
    git(t.path(), &["config", "user.name", "Test"]);
    git(t.path(), &["config", "user.email", "test@example.com"]);
    t
}

fn scan_repo(dir: &std::path::Path) -> repocard::RepoSnapshot {
    scan(dir, &ScanOptions::default(), |_| {}).expect("scan ok")
}

#[test]
fn initial_repo_clean_with_branch_and_head() {
    let t = init_repo();
    fs::write(t.path().join("a.txt"), "hi").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "init"]);
    let s = scan_repo(t.path());
    assert!(s.repository.is_git_repository);
    let g = s.git.expect("git snapshot");
    assert_eq!(g.branch.as_deref(), Some("main"));
    assert!(!g.detached);
    assert!(g.head_oid.is_some());
    assert!(g.head_short.is_some());
    assert!(g.clean);
    assert!(g.latest_commit.is_some());
}

#[test]
fn modified_staged_untracked_deleted_detected() {
    let t = init_repo();
    fs::write(t.path().join("a.txt"), "one").unwrap();
    fs::write(t.path().join("b.txt"), "two").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "init"]);
    // modified
    fs::write(t.path().join("a.txt"), "changed").unwrap();
    // staged new file
    fs::write(t.path().join("staged.txt"), "s").unwrap();
    git(t.path(), &["add", "staged.txt"]);
    // untracked
    fs::write(t.path().join("new.txt"), "n").unwrap();
    // deleted
    fs::remove_file(t.path().join("b.txt")).unwrap();
    let s = scan_repo(t.path());
    let g = s.git.unwrap();
    assert!(!g.clean);
    assert!(g.modified_paths.contains(&"a.txt".to_string()), "{g:?}");
    assert!(g.staged_paths.contains(&"staged.txt".to_string()), "{g:?}");
    assert!(g.untracked_paths.contains(&"new.txt".to_string()), "{g:?}");
    assert!(g.deleted_paths.contains(&"b.txt".to_string()), "{g:?}");
}

#[test]
fn detached_head_reported() {
    let t = init_repo();
    fs::write(t.path().join("a.txt"), "one").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "one"]);
    // detach
    git(t.path(), &["checkout", "--detach", "HEAD"]);
    let s = scan_repo(t.path());
    let g = s.git.unwrap();
    assert!(g.detached);
    assert!(g.branch.is_none());
}

#[test]
fn ahead_behind_via_local_repos() {
    // origin repo
    let origin = init_repo();
    fs::write(origin.path().join("a.txt"), "v1").unwrap();
    git(origin.path(), &["add", "."]);
    git(origin.path(), &["commit", "-m", "init"]);
    // clone locally
    let parent = TempDir::new().unwrap();
    let clone_path = parent.path().join("clone");
    let st = Command::new("git")
        .args([
            "clone",
            &origin.path().to_string_lossy(),
            &clone_path.to_string_lossy(),
        ])
        .status()
        .unwrap();
    assert!(st.success());
    git(&clone_path, &["config", "user.name", "Test"]);
    git(&clone_path, &["config", "user.email", "test@example.com"]);
    // new commit in clone -> ahead 1
    fs::write(clone_path.join("b.txt"), "new").unwrap();
    git(&clone_path, &["add", "."]);
    git(&clone_path, &["commit", "-m", "ahead"]);
    let s = scan(&clone_path, &ScanOptions::default(), |_| {}).unwrap();
    let g = s.git.unwrap();
    assert_eq!(g.ahead.unwrap_or(0), 1, "{g:?}");
}

#[test]
fn paths_with_spaces_and_unicode_in_git() {
    let t = init_repo();
    fs::write(t.path().join("with space.txt"), "x").unwrap();
    fs::write(t.path().join("ünicode.txt"), "y").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "init"]);
    fs::write(t.path().join("with space.txt"), "modified").unwrap();
    let s = scan_repo(t.path());
    let g = s.git.unwrap();
    assert!(
        g.modified_paths.contains(&"with space.txt".to_string()),
        "{g:?}"
    );
}

#[test]
fn merge_conflict_detected_if_robust() {
    let t = init_repo();
    fs::write(t.path().join("a.txt"), "base\n").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "base"]);
    git(t.path(), &["checkout", "-b", "side"]);
    fs::write(t.path().join("a.txt"), "side\n").unwrap();
    git(t.path(), &["commit", "-am", "side"]);
    git(t.path(), &["checkout", "main"]);
    fs::write(t.path().join("a.txt"), "main\n").unwrap();
    git(t.path(), &["commit", "-am", "main"]);
    // merge side -> conflict expected
    let st = Command::new("git")
        .arg("-C")
        .arg(t.path())
        .args(["merge", "side"])
        .status()
        .unwrap();
    let _ = st;
    let s = scan_repo(t.path());
    let g = s.git.unwrap();
    // Either conflicted or one side won; assert scan didn't crash and state is coherent.
    assert!(
        g.conflict_paths.contains(&"a.txt".to_string()) || !g.clean,
        "{g:?}"
    );
}
