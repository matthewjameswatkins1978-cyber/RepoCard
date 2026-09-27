pub mod attention;
pub mod files;
pub mod git;
pub mod history;
pub mod languages;
pub mod project;

use crate::model::{
    absolutize, relative_display, RepoCardError, RepoSnapshot, RepositoryIdentity, ScanEvent,
    ScanOptions, ScanPhase, WarningSink, SCHEMA_VERSION,
};
use std::path::{Path, PathBuf};

pub use git::GitRunner;

/// Scan a directory and produce a RepoSnapshot.
///
/// `on_event` is a simple semantic progress sink (the future Sartorial seam).
pub fn scan<E>(
    path: &Path,
    options: &ScanOptions,
    mut on_event: E,
) -> Result<RepoSnapshot, RepoCardError>
where
    E: FnMut(ScanEvent),
{
    let emit = |phase: ScanPhase, message: &str, on_event: &mut E| {
        on_event(ScanEvent {
            phase,
            current: 0,
            total: None,
            message: message.to_string(),
        });
    };

    // Resolve root.
    let raw = path;
    let abs = absolutize(raw);
    let canon = std::fs::canonicalize(&abs)
        .map_err(|_| RepoCardError::NotFound(abs.to_string_lossy().into_owned()))?;
    let meta = std::fs::metadata(&canon)
        .map_err(|_| RepoCardError::Unreadable(canon.to_string_lossy().into_owned()))?;
    if !meta.is_dir() {
        return Err(RepoCardError::Scan(format!(
            "not a directory: {}",
            canon.to_string_lossy()
        )));
    }
    let root: PathBuf = canon.clone();
    let mut warnings = WarningSink::new(options.warnings_limit);

    emit(ScanPhase::Discover, "discover", &mut on_event);

    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let root_str = root.to_string_lossy().into_owned();

    // ---- Git ----
    emit(ScanPhase::Git, "git", &mut on_event);
    let runner = GitRunner::default();
    scan_with_runner(
        &root,
        &name,
        &root_str,
        options,
        &runner,
        &mut on_event,
        &mut warnings,
    )
}

pub fn scan_with_runner<E>(
    root: &Path,
    name: &str,
    root_str: &str,
    options: &ScanOptions,
    runner: &GitRunner,
    on_event: &mut E,
    warnings: &mut WarningSink,
) -> Result<RepoSnapshot, RepoCardError>
where
    E: FnMut(ScanEvent),
{
    let mut emit = |phase: ScanPhase, message: &str| {
        on_event(ScanEvent {
            phase,
            current: 0,
            total: None,
            message: message.to_string(),
        });
    };

    // Git (optional).
    emit(ScanPhase::Git, "git");
    let git = match git::scan_git(root, runner, warnings) {
        Some(g) => Some(g),
        None => {
            // Distinguish "not a repo / git absent" from hard failure:
            // scan_git already warned on failure; add factual warning only if
            // it looks like git is absent or not a repo.
            let resolve = git::resolve_repo(root, runner);
            if !resolve.is_repo {
                warnings.push(
                    "git",
                    None,
                    "git unavailable or not a git repository; continuing with filesystem scan"
                        .to_string(),
                );
            }
            None
        }
    };
    let is_git = git.is_some() || git::resolve_repo(root, runner).is_repo;

    // ---- Walk ----
    emit(ScanPhase::Walk, "walk");
    let walk = files::walk_source_tree(
        root,
        options.follow_symlinks,
        options.largest_files_limit,
        options.large_file_threshold_bytes,
        warnings,
    );
    let rel_paths: Vec<String> = walk.files.iter().map(|f| f.rel_path.clone()).collect();
    let test_files = files::detect_test_files(&rel_paths);
    let git_dir_bytes = files::git_dir_bytes(root, warnings);

    // ---- Languages ----
    emit(ScanPhase::Languages, "languages");
    let languages = languages::analyze_languages(root, warnings);

    // ---- Attention ----
    emit(ScanPhase::Attention, "attention");
    let attention = attention::scan_attention(
        &walk.files,
        options.content_scan_max_bytes,
        options.attention_locations_limit,
        warnings,
    );

    // ---- History ----
    emit(ScanPhase::History, "history");
    let history = history::analyze_history(
        root,
        runner,
        options.history_commits,
        options.hot_files_limit,
        options.history_enabled,
        warnings,
    );

    // ---- Project ----
    emit(ScanPhase::Project, "project");
    let project = project::detect_project(root);

    emit(ScanPhase::Finalize, "finalize");

    // Warn on lossy root rendering.
    if root.as_os_str().to_str().is_none() {
        warnings.push(
            "path",
            None,
            "repository root contains non-Unicode data; rendered lossily".to_string(),
        );
    }
    let _ = relative_display;

    let partial = warnings.is_partial();

    Ok(RepoSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        repository: RepositoryIdentity {
            name: name.to_string(),
            root: root_str.to_string(),
            is_git_repository: is_git,
        },
        git,
        filesystem: crate::model::FilesystemSnapshot {
            file_count: walk.file_count,
            scanned_bytes: walk.scanned_bytes,
            largest_files: walk.largest_files,
            large_file_threshold_bytes: options.large_file_threshold_bytes,
            large_files_over_threshold: walk.large_files_over_threshold,
            git_directory_bytes: git_dir_bytes,
        },
        languages,
        tests: crate::model::TestFootprint {
            files: test_files,
            heuristic: true,
        },
        attention,
        history,
        project,
        warnings: std::mem::take(&mut warnings.warnings),
        suppressed_warnings: warnings.suppressed,
        partial,
    })
}
