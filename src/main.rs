use clap::{Parser, Subcommand};
use repocard::{report, scan::scan, ScanOptions};
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

    /// Emit only JSON to stdout
    #[arg(long)]
    json: bool,

    /// Expanded plain view
    #[arg(long)]
    details: bool,

    #[command(subcommand)]
    command: Option<Commands>,
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
        }) => cmd_write(path, output, dry_run, force),
        None => cmd_scan(cli.path, cli.json, cli.details),
    }
}

fn cmd_scan(path: PathBuf, as_json: bool, details: bool) {
    let options = ScanOptions::default();
    let snapshot = match scan(&path, &options, |_ev| {}) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("repocard: error: {e}");
            std::process::exit(1);
        }
    };
    if as_json {
        match serde_json::to_string_pretty(&snapshot) {
            Ok(j) => println!("{j}"),
            Err(e) => {
                eprintln!("repocard: serialize error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if details {
        print!(
            "{}",
            repocard::presentation::plain::render_details(&snapshot)
        );
    } else {
        print!(
            "{}",
            repocard::presentation::plain::render_compact(&snapshot)
        );
    }
}

fn cmd_write(path: PathBuf, output: Option<PathBuf>, dry_run: bool, force: bool) {
    // Dry-run skips mutation, not validation: resolve the source root with the
    // same semantics as a real write so a missing/unreadable source can never
    // yield a plausible-looking plan.
    let canon = match repocard::scan::resolve_scan_root(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("repocard: error: {e}");
            std::process::exit(1);
        }
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
        println!("ReportPlan:");
        println!("  destination: {}", plan.destination);
        println!("  will_create_directory: {}", plan.will_create_directory);
        println!("  will_create_file: {}", plan.will_create_file);
        println!("  will_overwrite: {}", plan.will_overwrite);
        println!("  schema_version: {}", plan.schema_version);
        println!("  (dry-run: wrote nothing)");
        return;
    }
    let options = ScanOptions::default();
    let snapshot = match scan(&path, &options, |_ev| {}) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("repocard: error: {e}");
            std::process::exit(1);
        }
    };
    match report::write_report(&snapshot, &dest_abs, force) {
        Ok(receipt) => {
            println!("ReportReceipt:");
            println!("  destination: {}", receipt.destination);
            println!("  bytes_written: {}", receipt.bytes_written);
            println!("  schema_version: {}", receipt.schema_version);
        }
        Err(e) => {
            eprintln!("repocard: error: {e}");
            std::process::exit(1);
        }
    }
}
