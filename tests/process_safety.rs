use repocard::scan::GitRunner;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Serialise daemon spawns: parallel `taskkill /T` tree-kills plus Windows
/// PID reuse must never let one test's cleanup touch another test's child.
static DAEMON_LOCK: Mutex<()> = Mutex::new(());

/// A hanging child must be killed and reaped: `run` returns a timeout error
/// promptly and no Git process remains.
///
/// The hanger is `git daemon`, which blocks serving until killed. This keeps
/// the test portable (no per-OS sleeper binary) and exercises the real
/// `GitRunner` argv path. `--port=0` asks the kernel for an ephemeral port so
/// parallel/loaded CI machines do not collide.
fn daemon_runner(timeout: Duration) -> GitRunner {
    GitRunner {
        program: "git".to_string(),
        timeout,
    }
}

fn daemon_args(base: &std::path::Path) -> Vec<String> {
    vec![
        "daemon".to_string(),
        "--port=0".to_string(),
        "--export-all".to_string(),
        format!("--base-path={}", base.to_string_lossy()),
    ]
}

#[test]
fn timeout_kills_child_and_returns_promptly() {
    let _guard = DAEMON_LOCK.lock().unwrap();
    let t = TempDir::new().unwrap();
    let runner = daemon_runner(Duration::from_millis(500));
    let args = daemon_args(t.path());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let start = Instant::now();
    let err = runner
        .run(t.path(), &arg_refs)
        .expect_err("blocking daemon must time out");
    let elapsed = start.elapsed();
    assert!(err.contains("timed out"), "unexpected error: {err}");
    // A serving daemon would run forever: generous bound for loaded CI.
    assert!(
        elapsed < Duration::from_secs(20),
        "timeout did not kill child promptly: {elapsed:?}"
    );
}

#[test]
fn timeout_leaves_runner_usable() {
    // After a kill+reap cycle the same runner must still work (no wedged
    // state). Only one daemon is ever launched here; the follow-ups are fast
    // deterministic commands, so this cannot flake on a second spawn.
    let _guard = DAEMON_LOCK.lock().unwrap();
    let t = TempDir::new().unwrap();
    let runner = daemon_runner(Duration::from_millis(300));
    let args = daemon_args(t.path());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    assert!(runner.run(t.path(), &arg_refs).is_err());
    let version = runner
        .run(t.path(), &["--version"])
        .expect("runner usable after timeout");
    assert!(version.success);
    let missing = GitRunner {
        program: "repocard-nonexistent-git-binary-xyz".to_string(),
        timeout: Duration::from_secs(5),
    };
    // Missing binary still reports spawn failure, not a hang.
    let err = missing.run(t.path(), &[]).expect_err("must fail");
    assert!(err.contains("cannot spawn"), "unexpected error: {err}");
}
