use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Schema version for RepoCard v0.1 snapshots.
pub const SCHEMA_VERSION: &str = "repocard.v0.1";

/// Explicit scan configuration. Keep as API config; CLI stays simple.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub history_commits: usize,
    pub hot_files_limit: usize,
    pub largest_files_limit: usize,
    pub large_file_threshold_bytes: u64,
    pub content_scan_max_bytes: u64,
    pub attention_locations_limit: usize,
    pub warnings_limit: usize,
    pub follow_symlinks: bool,
    pub history_enabled: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            history_commits: 200,
            hot_files_limit: 10,
            largest_files_limit: 10,
            large_file_threshold_bytes: 1024 * 1024,
            content_scan_max_bytes: 2 * 1024 * 1024,
            attention_locations_limit: 100,
            warnings_limit: 50,
            follow_symlinks: false,
            history_enabled: true,
        }
    }
}

/// Semantic scan progress event. No presentation data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanEvent {
    pub phase: ScanPhase,
    pub current: u64,
    pub total: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Discover,
    Git,
    Walk,
    Languages,
    Attention,
    History,
    Project,
    Finalize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanWarning {
    pub category: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoSnapshot {
    pub schema_version: String,
    pub repository: RepositoryIdentity,
    pub git: Option<GitSnapshot>,
    pub filesystem: FilesystemSnapshot,
    pub languages: LanguageSnapshot,
    pub tests: TestFootprint,
    pub attention: AttentionSnapshot,
    pub history: HistorySnapshot,
    pub project: ProjectSnapshot,
    pub warnings: Vec<ScanWarning>,
    pub suppressed_warnings: u64,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryIdentity {
    pub name: String,
    pub root: String,
    pub is_git_repository: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitSnapshot {
    pub branch: Option<String>,
    pub detached: bool,
    pub head_oid: Option<String>,
    pub head_short: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub staged_paths: Vec<String>,
    pub modified_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub renamed_paths: Vec<String>,
    pub untracked_paths: Vec<String>,
    pub conflict_paths: Vec<String>,
    pub stash_count: u64,
    pub clean: bool,
    pub latest_commit: Option<LatestCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatestCommit {
    pub oid: String,
    pub short_oid: String,
    pub committed_at: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilesystemSnapshot {
    pub file_count: u64,
    pub scanned_bytes: u64,
    pub largest_files: Vec<LargestFile>,
    pub large_file_threshold_bytes: u64,
    pub large_files_over_threshold: u64,
    pub git_directory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LargestFile {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageSnapshot {
    pub total_code_lines: u64,
    pub total_comment_lines: u64,
    pub total_blank_lines: u64,
    pub languages: Vec<LanguageStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageStat {
    pub language: String,
    pub files: u64,
    pub code: u64,
    pub comments: u64,
    pub blanks: u64,
    pub code_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestFootprint {
    pub files: Vec<String>,
    pub heuristic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionSnapshot {
    pub todo_count: u64,
    pub fixme_count: u64,
    pub hack_count: u64,
    pub xxx_count: u64,
    pub locations: Vec<AttentionLocation>,
    pub truncated: bool,
    pub scanned_text_files: u64,
    pub skipped_large_text_files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionLocation {
    pub marker: String,
    pub path: String,
    pub line: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistorySnapshot {
    pub commit_window: usize,
    pub commits_sampled: usize,
    pub hot_files: Vec<HotFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotFile {
    pub path: String,
    pub change_appearances: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectSnapshot {
    pub readme: Option<String>,
    pub licence: Option<String>,
    pub ci: Vec<String>,
    pub manifests: Vec<String>,
    pub guidance: Vec<String>,
}

/// Report plan for `repocard write --dry-run`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportPlan {
    pub destination: String,
    pub will_create_directory: bool,
    pub will_create_file: bool,
    pub will_overwrite: bool,
    pub schema_version: String,
}

/// Receipt after writing a report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportReceipt {
    pub destination: String,
    pub bytes_written: u64,
    pub schema_version: String,
}

/// Small explicit error model.
#[derive(Debug, thiserror::Error)]
pub enum RepoCardError {
    #[error("directory not found: {0}")]
    NotFound(String),
    #[error("directory unreadable: {0}")]
    Unreadable(String),
    #[error("scan failed: {0}")]
    Scan(String),
    #[error("report destination exists (use --force to overwrite): {0}")]
    Exists(String),
    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for RepoCardError {
    fn from(e: std::io::Error) -> Self {
        RepoCardError::Io(e.to_string())
    }
}

/// Internal warning accumulator with cap.
pub struct WarningSink {
    pub warnings: Vec<ScanWarning>,
    pub suppressed: u64,
    pub limit: usize,
}

impl WarningSink {
    pub fn new(limit: usize) -> Self {
        Self {
            warnings: Vec::new(),
            suppressed: 0,
            limit,
        }
    }

    pub fn push(&mut self, category: &str, path: Option<String>, message: String) {
        if self.warnings.len() < self.limit {
            self.warnings.push(ScanWarning {
                category: category.to_string(),
                path,
                message,
            });
        } else {
            self.suppressed += 1;
        }
    }

    pub fn is_partial(&self) -> bool {
        !self.warnings.is_empty() || self.suppressed > 0
    }
}

/// Repo-relative path with `/` separator, lossy fallback recorded by caller.
pub fn relative_display(root: &std::path::Path, path: &std::path::Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts: Vec<String> = Vec::new();
    for comp in rel.components() {
        match comp {
            std::path::Component::Normal(os) => parts.push(os.to_string_lossy().into_owned()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => parts.push("..".to_string()),
            _ => parts.push(comp.as_os_str().to_string_lossy().into_owned()),
        }
    }
    if parts.is_empty() {
        return Some(".".to_string());
    }
    Some(parts.join("/"))
}

/// Deterministic rounding to 1 decimal for language percentages.
pub fn code_percent(code: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let pct = (code as f64) * 100.0 / (total as f64);
    (pct * 10.0).round() / 10.0
}

/// Build a BTreeMap-sorted helper (keeps imports honest for future use).
#[allow(dead_code)]
pub fn sorted_counts(counts: BTreeMap<String, u64>) -> Vec<(String, u64)> {
    counts.into_iter().collect()
}

#[allow(dead_code)]
pub fn root_basename(root: &std::path::Path) -> String {
    root.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

/// Display form of the repository root for semantic output.
///
/// Keeps the canonical `PathBuf` internally (long-path support intact) but
/// strips the Windows extended-length `\\?\` implementation detail so users
/// see a normal path (`D:\repo`, or `\\server\share` for UNC).
pub fn display_root(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    s
}

pub fn absolutize(p: &std::path::Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    }
}
