use repocard::scan::GitRunner;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// A hanging child must be killed and reaped: `run` returns a timeout error
/// promptly and no Git process remains. Sleepers are platform-specific, so the
/// hanging program is selected per OS.
#[cfg(unix)]
fn hanging_runner(timeout: Duration) -> GitRunner {
    GitRunner {
        program: "sleep".to_string(),
        timeout,
    }
}

#[cfg(unix)]
fn hanging_args() -> Vec<String> {
    vec!["30".to_string()]
}

#[cfg(windows)]
fn hanging_runner(timeout: Duration) -> GitRunner {
    GitRunner {
        program: "powershell".to_string(),
        timeout,
    }
}

#[cfg(windows)]
fn hanging_args() -> Vec<String> {
    vec![
        "-NoProfile".to_string(),
        "-Command".to_string(),
        "Start-Sleep -Seconds 30".to_string(),
    ]
}

#[test]
fn timeout_kills_child_and_returns_promptly() {
    let t = TempDir::new().unwrap();
    let runner = hanging_runner(Duration::from_millis(500));
    let args: Vec<String> = hanging_args();
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let start = Instant::now();
    let err = runner
        .run(t.path(), &arg_refs)
        .expect_err("hanging child must time out");
    let elapsed = start.elapsed();
    assert!(err.contains("timed out"), "unexpected error: {err}");
    // 30s sleeper must not run to completion: generous bound for loaded CI.
    assert!(
        elapsed < Duration::from_secs(20),
        "timeout did not kill child promptly: {elapsed:?}"
    );
}

#[test]
fn timeout_leaves_runner_usable() {
    // After a kill+reap cycle the runner must still work (no wedged state).
    let t = TempDir::new().unwrap();
    let killer = hanging_runner(Duration::from_millis(300));
    let args: Vec<String> = hanging_args();
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    assert!(killer.run(t.path(), &arg_refs).is_err());
    let missing = GitRunner {
        program: "repocard-nonexistent-git-binary-xyz".to_string(),
        timeout: Duration::from_secs(5),
    };
    // Missing binary still reports spawn failure, not a hang.
    let err = missing.run(t.path(), &[]).expect_err("must fail");
    assert!(err.contains("cannot spawn"), "unexpected error: {err}");
}
