//! RepoCard presentation adapter over Sartorial.
//!
//! Boundary (kept deliberately thin):
//!
//! ```text
//! RepoSnapshot  -> SummaryScreen (+ detail tables)
//! ScanEvent     -> ProgressBar subtask update (Activity mode, unknown total)
//! ReportPlan    -> Plan
//! ReportReceipt -> Receipt
//! RepoCardError -> ErrorModel / ErrorView
//! ```
//!
//! The adapter only converts; it never rescans, never renames snapshot fields,
//! and never invents judgement (no health scores, no "dirty == broken").
//! Overall screen status describes scan completion only:
//! Ready = complete snapshot, Attention = partial snapshot.

use crate::model::{
    GitSnapshot, RepoCardError, RepoSnapshot, ReportPlan, ReportReceipt, ScanPhase,
};
use sartorial::{
    Config, ErrorModel, MotionMode, Notice, Plan, PlanChange, Preset, ProgressBar, RenderContext,
    RenderTarget, Status, SummaryScreen, TableModel,
};
use std::io::IsTerminal;

/// Build the render context RepoCard honours.
///
/// * `preset` selects one of the four Sartorial presets (House default).
/// * `motion` gates animated progress.
/// * `plain` forces pipe-safe output.
/// * Non-TTY stdout automatically selects the plain target so redirected
///   output stays readable; the preset still controls layout.
pub fn context(preset: Preset, motion: MotionMode, plain: bool) -> RenderContext {
    let config = Config::default().with_preset(preset).with_motion(motion);
    let ctx = RenderContext::detect().with_config(config);
    if plain || !std::io::stdout().is_terminal() {
        ctx.with_target(RenderTarget::Plain)
    } else {
        ctx
    }
}

/// Scan-completion status. Dirty trees, TODOs and hot files never escalate:
/// only a partial snapshot (missing requested information) maps to Attention.
pub fn completion_status(snapshot: &RepoSnapshot) -> Status {
    if snapshot.partial {
        Status::Attention
    } else {
        Status::Ready
    }
}

/// Human label for a scanner phase. Mirrors `ScanPhase` one-to-one; the
/// scanner remains the single authority for phase order and meaning.
pub fn phase_label(phase: ScanPhase) -> &'static str {
    match phase {
        ScanPhase::Discover => "discover",
        ScanPhase::Git => "git",
        ScanPhase::Walk => "walk",
        ScanPhase::Languages => "languages",
        ScanPhase::Attention => "attention",
        ScanPhase::History => "history",
        ScanPhase::Project => "project",
        ScanPhase::Finalize => "finalize",
    }
}

/// Fresh Activity progress for a scan. Total work is unknown, so this never
/// carries counts or percentages; phase updates only retitle the subtask.
pub fn scan_progress(display_root: &str) -> ProgressBar {
    ProgressBar::activity(format!("Scanning {display_root}")).with_subtask("discover")
}

/// Default (concise) summary screen: identity, repo state, code, attention,
/// recent activity, and partial-scan notices.
pub fn summary_screen(snapshot: &RepoSnapshot) -> SummaryScreen {
    build_screen(snapshot, false)
}

/// Details screen: the concise facts plus fuller git counts, project signals,
/// and a complete language table. Further bounded tables (largest files, hot
/// files, attention locations) come from [`detail_tables`].
pub fn details_screen(snapshot: &RepoSnapshot) -> SummaryScreen {
    build_screen(snapshot, true).with_table(language_table(snapshot, usize::MAX))
}

fn build_screen(snapshot: &RepoSnapshot, detailed: bool) -> SummaryScreen {
    let mut screen = SummaryScreen::new(
        format!("RepoCard: {}", snapshot.repository.name),
        completion_status(snapshot),
    )
    .with_subtitle(snapshot.repository.root.clone())
    .fact("Root", snapshot.repository.root.clone());

    screen = match &snapshot.git {
        Some(git) => push_git_facts(screen, git),
        None => screen.fact("Repository", "Directory (not a git repository)".to_string()),
    };

    screen = screen
        .fact(
            "Files",
            format!(
                "{} files · {} bytes",
                snapshot.filesystem.file_count, snapshot.filesystem.scanned_bytes
            ),
        )
        .fact("Languages", language_line(snapshot, 3))
        .fact("Test files", snapshot.tests.files.len().to_string())
        .fact(
            "Attention",
            format!(
                "TODO {} · FIXME {} · HACK {} · XXX {}",
                snapshot.attention.todo_count,
                snapshot.attention.fixme_count,
                snapshot.attention.hack_count,
                snapshot.attention.xxx_count
            ),
        )
        .fact("Recent", hot_line(snapshot, 3));

    if detailed {
        screen = with_detail_facts(screen, snapshot);
    }
    if let Some(git) = &snapshot.git {
        if !git.conflict_paths.is_empty() {
            screen = screen.notice(conflict_notice(git));
        }
    }
    if snapshot.partial {
        screen = screen.notice(partial_notice(snapshot, detailed));
    }
    screen
}

/// Extra bounded tables for `--details`, rendered after the summary screen:
/// largest files, hot files, and attention locations. All rows come straight
/// from the snapshot; nothing is re-derived here.
pub fn detail_tables(snapshot: &RepoSnapshot) -> Vec<TableModel> {
    let mut tables = Vec::new();
    if !snapshot.filesystem.largest_files.is_empty() {
        let mut t = TableModel::new(vec!["Largest file", "Bytes"]).with_title("Largest files");
        for f in &snapshot.filesystem.largest_files {
            t.add_row([f.path.clone(), f.bytes.to_string()]);
        }
        tables.push(t);
    }
    if !snapshot.history.hot_files.is_empty() {
        let mut t = TableModel::new(vec!["Hot file", "Changes"]).with_title("Hot files");
        for h in &snapshot.history.hot_files {
            t.add_row([h.path.clone(), h.change_appearances.to_string()]);
        }
        tables.push(t);
    }
    if !snapshot.attention.locations.is_empty() {
        let mut t = TableModel::new(vec!["Marker", "Location"]).with_title("Attention locations");
        for l in &snapshot.attention.locations {
            t.add_row([l.marker.clone(), format!("{}:{}", l.path, l.line)]);
        }
        tables.push(t);
    }
    tables
}

/// Dry-run plan view. Built from the actual [`ReportPlan`]; rendering it
/// must never write anything.
pub fn plan_view(plan: &ReportPlan) -> Plan {
    let mut view =
        Plan::new("Report plan").with_description("Dry run — no files were written.".to_string());
    if plan.will_create_directory {
        if let Some(parent) = std::path::Path::new(&plan.destination).parent() {
            let parent = parent.to_string_lossy().into_owned();
            if !parent.is_empty() {
                view = view.add_change(
                    PlanChange::add(parent).with_detail("Create directory".to_string()),
                );
            }
        }
    }
    let file_detail = if plan.will_overwrite {
        "Overwrite existing report"
    } else {
        "Write new report"
    };
    view = view
        .add_change(PlanChange::add(plan.destination.clone()).with_detail(file_detail.to_string()));
    if plan.will_overwrite {
        view = view
            .warning("The existing report at the destination will be replaced.".to_string())
            .consequence("Previous report contents will be replaced on write.".to_string());
    }
    view = view.consequence(format!("Schema version: {}", plan.schema_version));
    view
}

/// Success receipt view. Built from the actual [`ReportReceipt`] returned by
/// the write; nothing is reconstructed or guessed.
pub fn receipt_view(receipt: &ReportReceipt) -> sartorial::Receipt {
    sartorial::Receipt::success("Report written")
        .change("Destination", receipt.destination.clone())
        .change("Bytes written", receipt.bytes_written.to_string())
        .change("Schema version", receipt.schema_version.clone())
}

/// Fatal-error view: what happened, why when genuinely known, and the next
/// action inline when one is actually known (no fake key bindings).
pub fn error_view(error: &RepoCardError) -> sartorial::ErrorView {
    let model = match error {
        RepoCardError::NotFound(path) => {
            ErrorModel::new("Scan root not found").with_why(format!("Directory not found: {path}"))
        }
        RepoCardError::Unreadable(path) => ErrorModel::new("Scan root unreadable")
            .with_why(format!("Directory unreadable: {path}")),
        RepoCardError::Scan(detail) => {
            ErrorModel::new("Scan failed").with_why(format!("scan failed: {detail}"))
        }
        RepoCardError::Exists(dest) => ErrorModel::new("Report destination exists").with_why(
            format!("Report destination exists: {dest}. Rerun with --force to overwrite."),
        ),
        RepoCardError::Io(detail) => {
            ErrorModel::new("Report write failed").with_why(format!("io error: {detail}"))
        }
    };
    sartorial::ErrorView::new(model)
}

fn push_git_facts(screen: SummaryScreen, git: &GitSnapshot) -> SummaryScreen {
    let branch = git
        .branch
        .clone()
        .unwrap_or_else(|| "(detached)".to_string());
    let mut screen = screen
        .fact("Branch", branch)
        .fact(
            "Head",
            git.head_short.clone().unwrap_or_else(|| "-".to_string()),
        )
        .fact("Working tree", working_tree_line(git));
    if let Some(upstream) = &git.upstream {
        screen = screen.fact("Upstream", upstream.clone());
    }
    match (git.ahead, git.behind) {
        (Some(a), Some(b)) if a > 0 || b > 0 => {
            screen = screen.fact("Ahead/behind", format!("{a} ahead · {b} behind"));
        }
        _ => {}
    }
    if git.detached {
        screen = screen.fact("Detached", "yes".to_string());
    }
    screen
}

/// Extra git counts for details mode. The concise identity facts are already
/// present, so only counts are added here — details never repeats itself.
fn push_git_detail_facts(screen: SummaryScreen, git: &GitSnapshot) -> SummaryScreen {
    let screen = screen
        .fact("Staged", git.staged_paths.len().to_string())
        .fact("Modified", git.modified_paths.len().to_string())
        .fact("Deleted", git.deleted_paths.len().to_string())
        .fact("Renamed", git.renamed_paths.len().to_string())
        .fact("Untracked", git.untracked_paths.len().to_string())
        .fact("Conflicts", git.conflict_paths.len().to_string())
        .fact("Stashes", git.stash_count.to_string());
    if let Some(commit) = &git.latest_commit {
        screen.fact(
            "Latest commit",
            format!("{} {}", commit.short_oid, commit.subject),
        )
    } else {
        screen
    }
}

fn with_detail_facts(mut screen: SummaryScreen, snapshot: &RepoSnapshot) -> SummaryScreen {
    if let Some(git) = &snapshot.git {
        screen = push_git_detail_facts(screen, git);
    }
    screen = screen
        .fact(
            "Code lines",
            format!(
                "{} code · {} comment · {} blank",
                snapshot.languages.total_code_lines,
                snapshot.languages.total_comment_lines,
                snapshot.languages.total_blank_lines
            ),
        )
        .fact(
            "Large files",
            format!(
                "{} over {} bytes",
                snapshot.filesystem.large_files_over_threshold,
                snapshot.filesystem.large_file_threshold_bytes
            ),
        );
    if let Some(bytes) = snapshot.filesystem.git_directory_bytes {
        screen = screen.fact("Git dir size", format!("{bytes} bytes"));
    }
    screen = screen
        .fact(
            "Attention scan",
            format!(
                "{} text files · {} skipped large",
                snapshot.attention.scanned_text_files, snapshot.attention.skipped_large_text_files
            ),
        )
        .fact(
            "History window",
            format!(
                "{} sampled of {}",
                snapshot.history.commits_sampled, snapshot.history.commit_window
            ),
        )
        .fact(
            "Project",
            format!(
                "readme {} · licence {} · ci {} · manifests {} · guidance {}",
                snapshot
                    .project
                    .readme
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                snapshot
                    .project
                    .licence
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                snapshot.project.ci.len(),
                snapshot.project.manifests.len(),
                snapshot.project.guidance.len()
            ),
        );
    if !snapshot.tests.files.is_empty() {
        let shown: Vec<String> = snapshot.tests.files.iter().take(5).cloned().collect();
        let mut value = shown.join(", ");
        if snapshot.tests.files.len() > shown.len() {
            value.push_str(&format!(
                " (+{} more)",
                snapshot.tests.files.len() - shown.len()
            ));
        }
        screen = screen.fact("Test files list", value);
    }
    screen
}

/// Neutral working-tree description: counts, never verdicts.
fn working_tree_line(git: &GitSnapshot) -> String {
    if git.clean {
        return "Clean".to_string();
    }
    let mut parts = Vec::new();
    if !git.staged_paths.is_empty() {
        parts.push(format!("{} staged", git.staged_paths.len()));
    }
    if !git.modified_paths.is_empty() {
        parts.push(format!("{} modified", git.modified_paths.len()));
    }
    if !git.deleted_paths.is_empty() {
        parts.push(format!("{} deleted", git.deleted_paths.len()));
    }
    if !git.renamed_paths.is_empty() {
        parts.push(format!("{} renamed", git.renamed_paths.len()));
    }
    if !git.untracked_paths.is_empty() {
        parts.push(format!("{} untracked", git.untracked_paths.len()));
    }
    if !git.conflict_paths.is_empty() {
        parts.push(format!("{} conflicts", git.conflict_paths.len()));
    }
    if parts.is_empty() {
        "Changed".to_string()
    } else {
        parts.join(" · ")
    }
}

fn language_line(snapshot: &RepoSnapshot, limit: usize) -> String {
    if snapshot.languages.languages.is_empty() {
        return "None detected".to_string();
    }
    let top: Vec<String> = snapshot
        .languages
        .languages
        .iter()
        .take(limit)
        .map(top_lang)
        .collect();
    format!(
        "{} languages · top {}",
        snapshot.languages.languages.len(),
        top.join(" · ")
    )
}

fn top_lang(stat: &crate::model::LanguageStat) -> String {
    format!("{} {:.1}%", stat.language, stat.code_percent)
}

fn language_table(snapshot: &RepoSnapshot, limit: usize) -> TableModel {
    let mut table =
        TableModel::new(vec!["Language", "Files", "Code", "Share"]).with_title("Languages");
    for stat in snapshot.languages.languages.iter().take(limit) {
        table.add_row([
            stat.language.clone(),
            stat.files.to_string(),
            stat.code.to_string(),
            format!("{:.1}%", stat.code_percent),
        ]);
    }
    table
}

fn hot_line(snapshot: &RepoSnapshot, limit: usize) -> String {
    if snapshot.history.hot_files.is_empty() {
        return "No history".to_string();
    }
    snapshot
        .history
        .hot_files
        .iter()
        .take(limit)
        .map(hot_entry)
        .collect::<Vec<_>>()
        .join(" · ")
}

fn hot_entry(hot: &crate::model::HotFile) -> String {
    format!("{} ({})", hot.path, hot.change_appearances)
}

fn conflict_notice(git: &GitSnapshot) -> Notice {
    let mut shown: Vec<String> = git.conflict_paths.iter().take(5).cloned().collect();
    if git.conflict_paths.len() > shown.len() {
        shown.push(format!(
            "(+{} more)",
            git.conflict_paths.len() - shown.len()
        ));
    }
    Notice::warning(format!(
        "Merge conflicts present: {} path{}",
        git.conflict_paths.len(),
        if git.conflict_paths.len() == 1 {
            ""
        } else {
            "s"
        }
    ))
    .with_detail(shown.join(", "))
}

/// Calm partial-scan notice. Concise mode summarises; details mode lists the
/// full bounded warning set. Warnings stay notices, never fatal errors.
fn partial_notice(snapshot: &RepoSnapshot, full: bool) -> Notice {
    if !full {
        let mut notice = Notice::warning(format!(
            "Partial scan: {} warning{} ({} suppressed)",
            snapshot.warnings.len(),
            if snapshot.warnings.len() == 1 {
                ""
            } else {
                "s"
            },
            snapshot.suppressed_warnings
        ));
        if let Some(first) = snapshot.warnings.first() {
            notice = notice.with_detail(format!("e.g. [{}] {}", first.category, first.message));
        }
        return notice;
    }
    let mut notice = Notice::warning(format!(
        "Scan warnings: {} shown, {} suppressed",
        snapshot.warnings.len(),
        snapshot.suppressed_warnings
    ));
    let detail = snapshot
        .warnings
        .iter()
        .map(|w| {
            format!(
                "[{}] {}: {}",
                w.category,
                w.path.clone().unwrap_or_else(|| "-".to_string()),
                w.message
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    if !detail.is_empty() {
        notice = notice.with_detail(detail);
    }
    notice
}
