pub mod model;
pub mod presentation;
pub mod report;
pub mod scan;

pub use model::{
    code_percent, display_root, AttentionLocation, AttentionSnapshot, FilesystemSnapshot,
    GitSnapshot, HistorySnapshot, HotFile, LanguageSnapshot, LanguageStat, LargestFile,
    LatestCommit, ProjectSnapshot, RepoCardError, RepoSnapshot, ReportPlan, ReportReceipt,
    RepositoryIdentity, ScanEvent, ScanOptions, ScanPhase, ScanWarning, TestFootprint,
    SCHEMA_VERSION,
};
pub use scan::{scan, GitAvailability, GitRunner};
