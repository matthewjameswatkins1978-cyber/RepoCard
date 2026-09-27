use clap::{Parser, Subcommand, ValueEnum};
use repocard::{
    presentation::sartorial as present,
    report,
    scan::{resolve_scan_root, scan},
    RepoCardError, RepoSnapshot, ScanOptions,
};
use sartorial::{MotionMode, Preset, SartorialOutput, Status, TableView};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "repocard",
    version,
    about = "Small, fast repository intelligence card"
)]
struct Cli {
    /// Repository or directory path (default: .)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Emit only RepoCard JSON to stdout (RepoSnapshot contract; ignores presentation flags)
    #[arg(long)]
    json: bool,

    /// Expanded view
    #[arg(long)]
    details: bool,

    /// Pipe-safe plain output, no ANSI, no animation
    #[arg(long)]
    plain: bool,

    /// Visual preset (House is default and design authority)
    #[arg(long, value_enum, default_value_t = StyleArg::House)]
    style: StyleArg,

    /// Motion policy for scan progress
    #[arg(long, value_enum, default_value_t = MotionArg::Auto)]
    motion: MotionArg,

    #[command(subcommand)]
    command: Option<Commands>,
}

/// The four Sartorial presets RepoCard honours. House remains default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum StyleArg {
    House,
    #[value(name = "black-tie")]
    BlackTie,
    Workwear,
    Studio,
}

impl StyleArg {
    fn preset(self) -> Preset {
        match self {
            StyleArg::House => Preset::House,
            StyleArg::BlackTie => Preset::BlackTie,
            StyleArg::Workwear => Preset::Workwear,
            StyleArg::Studio => Preset::Studio,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum MotionArg {
    Auto,
    Always,
    Never,
}

impl MotionArg {
    fn mode(self) -> MotionMode {
        match self {
            MotionArg::Auto => MotionMode::Auto,
            MotionArg::Always => MotionMode::Always,
            MotionArg::Never => MotionMode::Never,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Write a JSON report to .repocard/report.json
    Write {
        /// Repository path
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
        /// Pipe-safe plain output, no ANSI, no animation
        #[arg(long)]
        plain: bool,
        /// Visual preset (House is default and design authority)
        #[arg(long, value_enum, default_value_t = StyleArg::House)]
        style: StyleArg,
        /// Motion policy for scan progress
        #[arg(long, value_enum, default_value_t = MotionArg::Auto)]
        motion: MotionArg,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Write {
            path,
            output,
            dry_run,
            force,
            plain,
            style,
            motion,
        }) => cmd_write(path, output, dry_run, force, plain, style, motion),
        None => cmd_scan(
            cli.path,
            cli.json,
            cli.details,
            cli.plain,
            cli.style,
            cli.motion,
        ),
    }
}

fn cmd_scan(
    path: PathBuf,
    as_json: bool,
    details: bool,
    plain: bool,
    style: StyleArg,
    motion: MotionArg,
) {
    // Machine contract: JSON mode emits the RepoSnapshot only, on stdout,
    // with zero presentation or progress noise on stderr.
    if as_json {
        let options = ScanOptions::default();
        let snapshot = match scan(&path, &options, |_| {}) {
            Ok(s) => s,
            Err(e) => fatal_json(&e),
        };
        match serde_json::to_string_pretty(&snapshot) {
            Ok(j) => println!("{j}"),
            Err(e) => fatal_json(&RepoCardError::Scan(format!("serialize snapshot: {e}"))),
        }
        return;
    }

    let ctx = present::context(style.preset(), motion.mode(), plain);
    let display = path.to_string_lossy().into_owned();
    let mut progress = present::scan_progress(&display);
    let _ = progress.start_live(&ctx);
    let options = ScanOptions::default();
    let snapshot = match scan(&path, &options, |ev| {
        progress.update_subtask(present::phase_label(ev.phase));
        let _ = progress.update_live(&ctx);
    }) {
        Ok(s) => s,
        Err(e) => {
            let _ = progress.finish_live(Status::Failed, &ctx);
            fatal(&e, &ctx);
        }
    };
    let _ = progress.finish_live(present::completion_status(&snapshot), &ctx);
    render_snapshot(&snapshot, details, &ctx);
}

fn render_snapshot(snapshot: &RepoSnapshot, details: bool, ctx: &sartorial::RenderContext) {
    let screen = if details {
        present::details_screen(snapshot)
    } else {
        present::summary_screen(snapshot)
    };
    if SartorialOutput::print_result(&screen, ctx).is_err() {
        std::process::exit(1);
    }
    if details {
        for table in present::detail_tables(snapshot) {
            if SartorialOutput::print_result(&TableView::new(table), ctx).is_err() {
                std::process::exit(1);
            }
        }
    }
}

fn cmd_write(
    path: PathBuf,
    output: Option<PathBuf>,
    dry_run: bool,
    force: bool,
    plain: bool,
    style: StyleArg,
    motion: MotionArg,
) {
    let ctx = present::context(style.preset(), motion.mode(), plain);
    // Dry-run skips mutation, not validation: resolve the source root with the
    // same semantics as a real write so a missing/unreadable source can never
    // yield a plausible-looking plan.
    let canon = match resolve_scan_root(&path) {
        Ok(c) => c,
        Err(e) => fatal(&e, &ctx),
    };
    let dest = output.unwrap_or_else(|| report::default_destination(&canon));
    let dest_abs = if dest.is_absolute() {
        dest.clone()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&dest))
            .unwrap_or_else(|_| dest.clone())
    };
    // User-facing destination: same file, without the `\\?\` prefix detail.
    let dest_abs = PathBuf::from(repocard::display_root(&dest_abs));
    let plan = report::plan_report(&canon, &dest_abs);
    if dry_run {
        if SartorialOutput::print_result(&present::plan_view(&plan), &ctx).is_err() {
            std::process::exit(1);
        }
        return;
    }
    let display = path.to_string_lossy().into_owned();
    let mut progress = present::scan_progress(&display);
    let _ = progress.start_live(&ctx);
    let options = ScanOptions::default();
    let snapshot = match scan(&path, &options, |ev| {
        progress.update_subtask(present::phase_label(ev.phase));
        let _ = progress.update_live(&ctx);
    }) {
        Ok(s) => s,
        Err(e) => {
            let _ = progress.finish_live(Status::Failed, &ctx);
            fatal(&e, &ctx);
        }
    };
    let _ = progress.finish_live(present::completion_status(&snapshot), &ctx);
    match report::write_report(&snapshot, &dest_abs, force) {
        Ok(receipt) => {
            if SartorialOutput::print_result(&present::receipt_view(&receipt), &ctx).is_err() {
                std::process::exit(1);
            }
        }
        Err(e) => fatal(&e, &ctx),
    }
}

/// Fatal human error: Sartorial ErrorView on stderr, exit 1.
fn fatal(error: &RepoCardError, ctx: &sartorial::RenderContext) -> ! {
    let view = present::error_view(error);
    let _ = SartorialOutput::print_diagnostic(&view, ctx);
    std::process::exit(1);
}

/// Fatal machine error: plain `repocard: ...` line on stderr, exit 1.
/// JSON stdout stays parseable (or empty); no presentation types involved.
fn fatal_json(error: &RepoCardError) -> ! {
    eprintln!("repocard: error: {error}");
    std::process::exit(1);
}
