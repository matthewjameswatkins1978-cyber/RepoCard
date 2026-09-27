use crate::model::{relative_display, LargestFile, WarningSink};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WalkedFile {
    pub abs_path: PathBuf,
    pub rel_path: String,
    pub bytes: u64,
}

pub struct WalkOutcome {
    pub files: Vec<WalkedFile>,
    pub file_count: u64,
    pub scanned_bytes: u64,
    pub largest_files: Vec<LargestFile>,
    pub large_files_over_threshold: u64,
}

pub fn walk_source_tree(
    root: &Path,
    options_follow_symlinks: bool,
    largest_limit: usize,
    large_threshold: u64,
    warnings: &mut WarningSink,
) -> WalkOutcome {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .ignore(true)
        .follow_links(options_follow_symlinks)
        .require_git(false)
        .standard_filters(true);
    builder.filter_entry(|e| {
        let name = e.file_name();
        // Never descend into .git as ordinary source.
        if name == ".git" {
            return false;
        }
        // Skip .repocard report output dir if present.
        if e.depth() == 1 && name == ".repocard" {
            return false;
        }
        true
    });

    let mut files: Vec<WalkedFile> = Vec::new();
    let mut scanned_bytes: u64 = 0;

    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                warnings.push("walk", None, format!("walk error: {err}"));
                continue;
            }
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let abs = entry.path().to_path_buf();
        let rel = match relative_display(root, &abs) {
            Some(r) => r,
            None => {
                warnings.push(
                    "walk",
                    Some(abs.to_string_lossy().into_owned()),
                    "path escapes repository root; skipped".to_string(),
                );
                continue;
            }
        };
        // Detect lossy path rendering.
        if abs.to_str().is_none() {
            warnings.push(
                "walk",
                Some(rel.clone()),
                "non-Unicode path rendered lossily".to_string(),
            );
        }
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        scanned_bytes = scanned_bytes.saturating_add(bytes);
        files.push(WalkedFile {
            abs_path: abs,
            rel_path: rel,
            bytes,
        });
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    // Largest files: size desc, path asc for deterministic ties.
    let mut by_size = files.clone();
    by_size.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    let largest_files: Vec<LargestFile> = by_size
        .into_iter()
        .take(largest_limit)
        .map(|f| LargestFile {
            path: f.rel_path,
            bytes: f.bytes,
        })
        .collect();
    let large_files_over_threshold =
        files.iter().filter(|f| f.bytes > large_threshold).count() as u64;

    let file_count = files.len() as u64;

    WalkOutcome {
        files,
        file_count,
        scanned_bytes,
        largest_files,
        large_files_over_threshold,
    }
}

/// Heuristic test-file footprint over repo-relative paths.
pub fn detect_test_files(rel_paths: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for p in rel_paths {
        if is_test_path(p) {
            out.push(p.clone());
        }
    }
    out.sort();
    out
}

fn is_test_path(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    let path = lower.replace('\\', "/");
    let file_name = path.rsplit('/').next().unwrap_or(&path);

    // Directory signals.
    for seg in path.split('/') {
        if seg == "tests" || seg == "test" || seg == "__tests__" {
            return true;
        }
    }
    // Rust integration tests live under tests/.
    // Generic patterns:
    if file_name.ends_with("_test.go") {
        return true;
    }
    if file_name.starts_with("test_") && file_name.ends_with(".py") {
        return true;
    }
    if file_name.contains(".test.") {
        return true;
    }
    if file_name.contains(".spec.") {
        return true;
    }
    // Common explicit names.
    if file_name == "test.py" || file_name == "tests.py" {
        return true;
    }
    if file_name.starts_with("test") && file_name.ends_with(".js") {
        return true;
    }
    false
}

/// Cheap bounded .git directory size (top-level entries only + recursive walk with cap).
pub fn git_dir_bytes(root: &Path, warnings: &mut WarningSink) -> Option<u64> {
    let dotgit = root.join(".git");
    let meta = std::fs::symlink_metadata(&dotgit).ok()?;
    if meta.is_file() {
        // Worktree gitfile pointer; don't chase.
        return meta.len().into();
    }
    if !meta.is_dir() {
        return None;
    }
    let mut total: u64 = 0;
    let mut entries: u64 = 0;
    const MAX_ENTRIES: u64 = 50_000;
    let mut stack = vec![dotgit];
    while let Some(dir) = stack.pop() {
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                warnings.push(
                    "filesystem",
                    relative_display(root, &dir),
                    format!("cannot read .git entry: {e}"),
                );
                continue;
            }
        };
        for ent in rd {
            entries += 1;
            if entries > MAX_ENTRIES {
                warnings.push(
                    "filesystem",
                    Some(".git".to_string()),
                    "git directory size walk capped; size omitted".to_string(),
                );
                return None;
            }
            let ent = match ent {
                Ok(e) => e,
                Err(e) => {
                    warnings.push("filesystem", None, format!("git dir entry error: {e}"));
                    continue;
                }
            };
            let md = match ent.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if md.is_dir() {
                stack.push(ent.path());
            } else {
                total = total.saturating_add(md.len());
            }
        }
    }
    Some(total)
}
