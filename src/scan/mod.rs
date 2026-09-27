pub mod attention;
pub mod files;
pub mod git;
pub mod history;
pub mod languages;
pub mod project;

use crate::model::{
    absolutize, display_root, RepoCardError, RepoSnapshot, RepositoryIdentity, ScanEvent,
    ScanOptions, ScanPhase, WarningSink, SCHEMA_VERSION,
};
use std::path::{Path, PathBuf};

pub use git::{GitAvailability, GitRunner};

/// Validate a scan root with the same semantics everywhere (scan, dry-run,
/// real write): must exist, must be readable, must be a directory.
/// Returns the canonical internal path (long-path capable).
pub fn resolve_scan_root(path: &Path) -> Result<PathBuf, RepoCardError> {
    let abs = absolutize(path);
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
    Ok(canon)
}

/// Scan a directory and produce a RepoSnapshot.
///
/// `on_event` is a simple semantic progress sink (the future Sartorial seam).
/// Exactly one phase sequence is emitted:
/// Discover, Git, Walk, Languages, Attention, History, Project, Finalize.
pub fn scan<E>(
    path: &Path,
    options: &ScanOptions,
    mut on_event: E,
) -> Result<RepoSnapshot, RepoCardError>
where
    E: FnMut(ScanEvent),
{
    let root = resolve_scan_root(path)?;
    let mut warnings = WarningSink::new(options.warnings_limit);

    on_event(ScanEvent {
        phase: ScanPhase::Discover,
        current: 0,
        total: None,
        message: "discover".to_string(),
    });

    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    // Semantic/display root: canonical internally, no `\\?\` prefix outward.
    let root_str = display_root(&root);

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

    // ---- Git (optional; exactly one Git event per scan) ----
    emit(ScanPhase::Git, "git");
    let (git, is_git) = match git::availability(root, runner) {
        GitAvailability::Repo(_) => match git::scan_git(root, runner, warnings) {
            Some(g) => (Some(g), true),
            None => {
                warnings.push(
                    "git",
                    None,
                    "git repository detected but status could not be read; continuing with filesystem scan"
                        .to_string(),
                );
                (None, true)
            }
        },
        // An ordinary directory is a supported complete target: no warning,
        // and not partial merely for being non-Git.
        GitAvailability::NotRepo => (None, false),
        GitAvailability::Unavailable(detail) => {
            warnings.push(
                "git",
                None,
                format!("git unavailable ({detail}); continuing with filesystem scan"),
            );
            (None, false)
        }
    };

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
