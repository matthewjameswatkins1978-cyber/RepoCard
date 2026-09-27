use crate::model::{HistorySnapshot, HotFile, WarningSink};
use crate::scan::git::GitRunner;
use std::collections::HashMap;
use std::path::Path;

pub fn analyze_history(
    root: &Path,
    runner: &GitRunner,
    window: usize,
    limit: usize,
    enabled: bool,
    warnings: &mut WarningSink,
) -> HistorySnapshot {
    if !enabled || window == 0 {
        return HistorySnapshot {
            commit_window: window,
            commits_sampled: 0,
            hot_files: Vec::new(),
        };
    }
    let n = window.to_string();
    let out = match runner.run(
        root,
        &[
            "log",
            "-n",
            &n,
            "--name-only",
            "--format=",
            "-z",
            "--no-renames",
        ],
    ) {
        Ok(o) => o,
        Err(e) => {
            warnings.push("history", None, format!("git log failed: {e}"));
            return HistorySnapshot {
                commit_window: window,
                commits_sampled: 0,
                hot_files: Vec::new(),
            };
        }
    };
    if !out.success {
        // Not a repo or no commits: return empty without failing.
        return HistorySnapshot {
            commit_window: window,
            commits_sampled: 0,
            hot_files: Vec::new(),
        };
    }
    // With --format= and -z, output is NUL-separated paths; empty commits
    // contribute empty records. Count commits via `git rev-list --count`?
    // Simpler: count commits sampled via separate bounded command.
    let commits_sampled = count_commits(root, runner, window);

    let mut freq: HashMap<String, u64> = HashMap::new();
    for raw in out.stdout.split(|b| *b == 0) {
        if raw.is_empty() {
            continue;
        }
        // Strip possible leading newlines from --format= newline handling.
        let mut p = raw;
        while !p.is_empty() && (p[0] == b'\n' || p[0] == b'\r') {
            p = &p[1..];
        }
        if p.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(p).into_owned();
        // Normalize to `/` separators.
        let norm = s.replace('\\', "/");
        *freq.entry(norm).or_insert(0) += 1;
    }

    let mut hot: Vec<HotFile> = freq
        .into_iter()
        .map(|(path, change_appearances)| HotFile {
            path,
            change_appearances,
        })
        .collect();
    hot.sort_by(|a, b| {
        b.change_appearances
            .cmp(&a.change_appearances)
            .then_with(|| a.path.cmp(&b.path))
    });
    hot.truncate(limit);

    HistorySnapshot {
        commit_window: window,
        commits_sampled,
        hot_files: hot,
    }
}

fn count_commits(root: &Path, runner: &GitRunner, window: usize) -> usize {
    let n = window.to_string();
    let out = runner
        .run(root, &["rev-list", "--count", "-n", &n, "HEAD"])
        .ok();
    match out {
        Some(o) if o.success => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<usize>()
            .unwrap_or(0),
        _ => 0,
    }
}
