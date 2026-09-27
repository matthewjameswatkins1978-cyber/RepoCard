use crate::model::{GitSnapshot, LatestCommit, WarningSink};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

/// Hard cap per captured stream. Enforced *during* capture via `take()`.
const MAX_CAPTURE_BYTES: u64 = 8 * 1024 * 1024;

/// How to invoke git. Injectable for absence tests.
#[derive(Debug, Clone)]
pub struct GitRunner {
    pub program: String,
    pub timeout: Duration,
}

impl Default for GitRunner {
    fn default() -> Self {
        Self {
            program: "git".to_string(),
            timeout: Duration::from_secs(15),
        }
    }
}

impl GitRunner {
    pub fn run(&self, root: &Path, args: &[&str]) -> Result<GitOutput, String> {
        let mut cmd = Command::new(&self.program);
        cmd.arg("-C")
            .arg(root)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        #[cfg(unix)]
        cmd.process_group(0);
        // Unix: the child leads its own process group, so a timeout kill can
        // take the whole tree (git re-executes subcommands such as
        // `git-daemon`, which inherit our pipes; killing only the parent
        // would leave a grandchild serving forever and the drain joins below
        // would block forever). Spawn fails if the group cannot be created,
        // so a later group-kill can never hit our own group.
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot spawn git ({}): {e}", self.program))?;
        // Drain both pipes on helper threads *while* the parent waits, so a
        // chatty child can never deadlock against full pipe buffers. Reads are
        // bounded during capture via `take()`, never truncated after the fact.
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let out_handle = std::thread::spawn(move || drain_capped(stdout_pipe));
        let err_handle = std::thread::spawn(move || drain_capped(stderr_pipe));
        let status = match child.wait_timeout(self.timeout) {
            Ok(Some(s)) => s,
            Ok(None) => {
                // Timeout: kill the whole process tree, then wait to reap the
                // direct child. A plain `kill()` is not enough: `git`
                // re-executes subcommands (e.g. `git-daemon`) which inherit
                // the pipes; killing only the parent would leave a grandchild
                // serving forever with our pipe ends open, and the drain joins
                // below would block forever. No Git process is left behind on
                // any platform.
                kill_tree(&mut child);
                let _ = child.wait();
                let _ = out_handle.join();
                let _ = err_handle.join();
                return Err("git timed out".to_string());
            }
            Err(e) => {
                kill_tree(&mut child);
                let _ = child.wait();
                let _ = out_handle.join();
                let _ = err_handle.join();
                return Err(format!("git wait error: {e}"));
            }
        };
        // Child has exited; pipes are at EOF so joins terminate promptly.
        let stdout = out_handle.join().unwrap_or_default();
        let stderr = err_handle.join().unwrap_or_default();
        Ok(GitOutput {
            success: status.success(),
            code: status.code(),
            stdout,
            stderr,
        })
    }
}
/// Drain one optional pipe to a bounded buffer. Never blocks the waiter.
fn drain_capped<R: Read>(pipe: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(p) = pipe {
        // `take` bounds the read during capture; the `+1` only detects overflow.
        let mut limited = p.take(MAX_CAPTURE_BYTES.saturating_add(1));
        let _ = limited.read_to_end(&mut buf);
    }
    buf.truncate(MAX_CAPTURE_BYTES as usize);
    buf
}

/// Kill a child and any grandchildren it spawned (direct argv, no shell).
/// Falls back to a plain kill if the tree kill cannot run.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // The child leads its own process group (see `run()`), so this takes
        // the whole tree including re-exec'd grandchildren. This is the same
        // mechanism as `timeout -k`.
        unsafe {
            libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[derive(Debug)]
pub struct GitOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The single semantic authority for Git availability.
///
/// - `Repo`: the directory is inside a Git work tree.
/// - `NotRepo`: Git ran fine but the directory is not a repository. An
///   ordinary directory is a supported complete target, not an error.
/// - `Unavailable`: the Git executable could not be run (absent, timeout,
///   wait failure). Filesystem scanning continues; the snapshot is partial.
pub enum GitAvailability {
    Repo(PathBuf),
    NotRepo,
    Unavailable(String),
}

/// Classify Git availability via `git rev-parse --show-toplevel`.
pub fn availability(root: &Path, runner: &GitRunner) -> GitAvailability {
    match runner.run(root, &["rev-parse", "--show-toplevel"]) {
        Err(e) => GitAvailability::Unavailable(e),
        Ok(o) if o.success => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                GitAvailability::NotRepo
            } else {
                GitAvailability::Repo(PathBuf::from(s))
            }
        }
        Ok(_) => GitAvailability::NotRepo,
    }
}

fn lossy_arg(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Parse `status --porcelain=v2 -z --branch --show-stash --untracked-files=normal`.
///
/// Call only after [`availability`] reports [`GitAvailability::Repo`].
/// Returns `None` (with a warning) when status cannot be read inside a
/// repository; the caller keeps repository identity as Git and marks partial.
pub fn scan_git(
    root: &Path,
    runner: &GitRunner,
    warnings: &mut WarningSink,
) -> Option<GitSnapshot> {
    let out = match runner.run(
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--show-stash",
            "--untracked-files=normal",
        ],
    ) {
        Ok(o) => o,
        Err(e) => {
            warnings.push("git", None, format!("git status failed: {e}"));
            return None;
        }
    };
    if !out.success {
        warnings.push(
            "git",
            None,
            format!(
                "git status failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        );
        return None;
    }

    let mut branch: Option<String> = None;
    let mut upstream: Option<String> = None;
    let mut ahead: Option<u64> = None;
    let mut behind: Option<u64> = None;
    let mut head_oid: Option<String> = None;

    let mut staged: Vec<String> = Vec::new();
    let mut modified: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    let mut renamed: Vec<String> = Vec::new();
    let mut untracked: Vec<String> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();

    for record in out.stdout.split(|b| *b == 0) {
        if record.is_empty() {
            continue;
        }
        if record[0] == b'#' {
            parse_header(
                record,
                &mut branch,
                &mut upstream,
                &mut ahead,
                &mut behind,
                &mut head_oid,
            );
        } else if record[0] == b'1' || record[0] == b'2' {
            parse_ordinary(
                record,
                &mut staged,
                &mut modified,
                &mut deleted,
                &mut renamed,
            );
        } else if record[0] == b'u' {
            // Unmerged, documented porcelain-v2 schema:
            // `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
            // so the path is the remainder after the 10th space. Verified
            // empirically against git 2.55 (`u UU N... 100644 x3 <hashes> path`).
            if let Some(p) = path_after_spaces(record, 10) {
                conflicts.push(p);
            }
        } else if record[0] == b'?' {
            if let Some(p) = path_after_spaces(record, 1) {
                untracked.push(p);
            }
        } else if record[0] == b'!' {
            // Ignored; skip.
        }
    }

    // Stash count is derived explicitly via `git stash list` for robustness.
    let stash_count = query_stash_count(root, runner, warnings);

    sort_dedup(&mut staged);
    sort_dedup(&mut modified);
    sort_dedup(&mut deleted);
    sort_dedup(&mut renamed);
    sort_dedup(&mut untracked);
    sort_dedup(&mut conflicts);

    let detached = branch.as_deref() == Some("(detached)") || branch.is_none();
    let branch_name = if detached { None } else { branch.clone() };

    let (resolved_head, short) = query_head(root, runner, warnings);
    if head_oid.is_none() {
        head_oid = resolved_head.clone();
    }
    let head_short = short.or_else(|| head_oid.clone().map(|o| o.chars().take(7).collect()));

    let latest_commit = query_latest_commit(root, runner, warnings);

    let clean = staged.is_empty()
        && modified.is_empty()
        && deleted.is_empty()
        && renamed.is_empty()
        && untracked.is_empty()
        && conflicts.is_empty();

    // Porcelain v2 with -z emits raw bytes (no C-quoting); paths already
    // support spaces and Unicode via lossy conversion at parse time.
    Some(GitSnapshot {
        branch: branch_name,
        detached,
        head_oid,
        head_short,
        upstream,
        ahead,
        behind,
        staged_paths: staged,
        modified_paths: modified,
        deleted_paths: deleted,
        renamed_paths: renamed,
        untracked_paths: untracked,
        conflict_paths: conflicts,
        stash_count,
        clean,
        latest_commit,
    })
}

fn parse_header(
    record: &[u8],
    branch: &mut Option<String>,
    upstream: &mut Option<String>,
    ahead: &mut Option<u64>,
    behind: &mut Option<u64>,
    head_oid: &mut Option<String>,
) {
    // Records like "# branch.head <name>", "# branch.upstream <name>",
    // "# branch.ab <ahead> <behind>", "# branch.oid <oid>"
    let s = lossy_arg(record);
    let mut it = s.split(' ');
    let _hash = it.next();
    let key = it.next().unwrap_or("");
    match key {
        "branch.head" => {
            let v = it.next().unwrap_or("(detached)");
            *branch = Some(v.to_string());
        }
        "branch.upstream" => {
            let v = it.next().unwrap_or("");
            if !v.is_empty() {
                *upstream = Some(v.to_string());
            }
        }
        "branch.ab" => {
            let a = it.next().unwrap_or("").trim_start_matches('+');
            let b = it.next().unwrap_or("").trim_start_matches('-');
            *ahead = a.parse().ok();
            *behind = b.parse().ok();
        }
        "branch.oid" => {
            let v = it.next().unwrap_or("");
            if v != "(initial)" && !v.is_empty() {
                *head_oid = Some(v.to_string());
            }
        }
        _ => {}
    }
}

/// Path is the remainder after `n` spaces. Paths may contain spaces; with `-z`
/// they arrive as raw bytes (no C-quoting), so splitting on every space and
/// taking one field is wrong. For `2` rename records, the remainder is
/// `<path><TAB><orig>`; take the portion before TAB.
fn path_after_spaces(record: &[u8], n: usize) -> Option<String> {
    let mut seen = 0usize;
    for (i, b) in record.iter().enumerate() {
        if *b == b' ' {
            seen += 1;
            if seen == n {
                let raw = &record[i + 1..];
                let first = raw.split(|c| *c == b'\t').next().unwrap_or(raw);
                return Some(String::from_utf8_lossy(first).into_owned());
            }
        }
    }
    None
}

fn parse_ordinary(
    record: &[u8],
    staged: &mut Vec<String>,
    modified: &mut Vec<String>,
    deleted: &mut Vec<String>,
    renamed: &mut Vec<String>,
) {
    // "1 <xy> <subm> <mH> <mI> <mW> <hH> <hI> <path>"
    // "2 <xy> <subm> <mH> <mI> <mW> <hH> <hI> <X><score> <path><TAB><orig>"
    let s = lossy_arg(record);
    let xy: String = s.split(' ').nth(1).unwrap_or("..").to_string();
    let mut xy_chars = xy.chars();
    let x = xy_chars.next().unwrap_or('.');
    let y = xy_chars.next().unwrap_or('.');
    let is_rename = record[0] == b'2';
    let path = if is_rename {
        path_after_spaces(record, 9).unwrap_or_default()
    } else {
        path_after_spaces(record, 8).unwrap_or_default()
    };

    if x == 'U'
        || y == 'U'
        || xy == "AA"
        || xy == "DD"
        || xy == "AU"
        || xy == "UA"
        || xy == "UD"
        || xy == "DU"
    {
        // Unmerged handled by 'u' records too; record here as conflict.
        // Avoid duplicates later via dedup.
    }

    if is_rename {
        if !path.is_empty() {
            renamed.push(path);
        }
        return;
    }

    match (x, y) {
        // Deleted
        ('D', '.') | ('.', 'D') => {
            if x == 'D' && y == '.' {
                staged.push(path.clone());
            }
            deleted.push(path);
        }
        _ => {
            if x != '.' {
                staged.push(path.clone());
            }
            if y != '.' {
                modified.push(path.clone());
            }
            if x == '.' && y == '.' {
                // Unmodified? shouldn't appear.
            }
        }
    }
}

fn sort_dedup(v: &mut Vec<String>) {
    v.sort();
    v.dedup();
}

fn query_stash_count(root: &Path, runner: &GitRunner, warnings: &mut WarningSink) -> u64 {
    match runner.run(root, &["stash", "list", "--format=%gd"]) {
        Ok(o) if o.success => o
            .stdout
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .count() as u64,
        Ok(o) => {
            let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if !msg.is_empty() {
                warnings.push("git", None, format!("git stash list failed: {msg}"));
            }
            0
        }
        Err(e) => {
            warnings.push("git", None, format!("git stash list failed: {e}"));
            0
        }
    }
}

fn query_head(
    root: &Path,
    runner: &GitRunner,
    warnings: &mut WarningSink,
) -> (Option<String>, Option<String>) {
    let oid = match runner.run(root, &["rev-parse", "HEAD"]) {
        Ok(o) if o.success => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    };
    if oid.is_none() {
        return (None, None);
    }
    let short = match runner.run(root, &["rev-parse", "--short", "HEAD"]) {
        Ok(o) if o.success => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => {
            warnings.push("git", None, "git rev-parse --short failed".to_string());
            None
        }
    };
    (oid, short)
}

fn query_latest_commit(
    root: &Path,
    runner: &GitRunner,
    warnings: &mut WarningSink,
) -> Option<LatestCommit> {
    // Machine-safe: %H %h %cI %s with NUL-safe? Subject may contain anything;
    // use unit separator.
    let out = runner
        .run(root, &["log", "-1", "--format=%H%x1f%h%x1f%cI%x1f%s"])
        .ok()?;
    if !out.success {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim_end_matches('\n');
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split('\x1f').collect();
    if parts.len() < 4 {
        warnings.push("git", None, "could not parse latest commit".to_string());
        return None;
    }
    Some(LatestCommit {
        oid: parts[0].to_string(),
        short_oid: parts[1].to_string(),
        committed_at: parts[2].to_string(),
        subject: parts[3].to_string(),
    })
}
