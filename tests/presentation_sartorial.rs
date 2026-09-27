//! RepoCard x Sartorial presentation integration tests.
//!
//! These test the presentation boundary only: every semantic fact comes from
//! an ordinary `scan()` snapshot, and the adapter only converts it into
//! Sartorial views. Scanner semantics are covered by the existing suite.

use repocard::presentation::sartorial as present;
use repocard::{scan, RepoSnapshot, ScanOptions};
use sartorial::{
    ColorChoice, Config, MotionMode, Preset, RenderContext, RenderTarget, Status, SymbolMode,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_repocard"))
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .arg(dir)
        .args(args)
        .output()
        .expect("spawn repocard")
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn plain_dir() -> TempDir {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("main.rs"), "fn main() {}\n// TODO: polish\n").unwrap();
    fs::write(t.path().join("notes.txt"), "FIXME later\n").unwrap();
    t
}

fn scan_snapshot(dir: &Path) -> RepoSnapshot {
    scan(dir, &ScanOptions::default(), |_| {}).expect("scan ok")
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(st.success(), "git {args:?} failed");
}

fn git_repo(dirty: bool) -> TempDir {
    let t = TempDir::new().unwrap();
    git(t.path(), &["init", "-b", "main"]);
    git(t.path(), &["config", "user.name", "Test"]);
    git(t.path(), &["config", "user.email", "test@example.com"]);
    fs::write(t.path().join("main.rs"), "fn main() {}\n").unwrap();
    git(t.path(), &["add", "."]);
    git(t.path(), &["commit", "-m", "init"]);
    if dirty {
        fs::write(t.path().join("main.rs"), "fn main() { todo!() }\n").unwrap();
        fs::write(t.path().join("scratch.txt"), "x\n").unwrap();
    }
    t
}

/// 1. Default House output contains the expected core RepoCard facts.
#[test]
fn default_house_output_contains_core_facts() {
    let t = plain_dir();
    let out = run(t.path(), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let root = repocard::display_root(&std::fs::canonicalize(t.path()).unwrap());
    for needle in [
        // Sartorial Title renders uppercase by design.
        "REPOCARD",
        root.as_str(),
        "Directory (not a git repository)",
        "Files",
        "Languages",
        "Test files",
        "Attention",
        "TODO 1",
        "FIXME 1",
        "Recent",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

/// 1b. Git facts appear for a real repository: branch, head, working tree.
#[test]
fn git_facts_appear_for_repository() {
    let t = git_repo(false);
    let out = run(t.path(), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    for needle in ["Branch", "main", "Head", "Working tree", "Clean"] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

/// 1c. Details mode exposes more of the snapshot without raw dumps.
#[test]
fn details_exposes_more_of_snapshot() {
    let t = git_repo(false);
    let out = run(t.path(), &["--details"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    for needle in [
        "Latest commit",
        "Code lines",
        "Largest files",
        "Languages",
        "Stashes",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
    assert!(!text.contains("GitSnapshot"), "raw struct leaked");
}

/// 2+3. `--json` remains valid RepoSnapshot JSON with silent stderr.
#[test]
fn json_is_snapshot_only_with_silent_stderr() {
    let t = plain_dir();
    let out = run(t.path(), &["--json"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(stderr(&out).is_empty(), "JSON stderr must be silent");
    let text = stdout(&out);
    assert!(!text.contains('\x1b'), "JSON must not contain ANSI");
    let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(v["schema_version"], "repocard.v0.1");
    assert!(v["repository"]["root"].is_string());
}

/// 4. `--plain` contains no ANSI and keeps the same facts.
#[test]
fn plain_has_no_ansi_and_keeps_facts() {
    let t = git_repo(false);
    let out = run(t.path(), &["--plain"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(!text.contains('\x1b'), "plain must not contain ANSI");
    for needle in ["Branch", "main", "Files", "Attention", "TODO"] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

/// 5. All four presets preserve the same essential semantic content (needles are preset-aware for Workwear operator voice).
#[test]
fn presets_preserve_semantic_content() {
    let t = git_repo(true);
    let root = repocard::display_root(&std::fs::canonicalize(t.path()).unwrap());
    for style in ["house", "black-tie", "workwear", "studio"] {
        let out = run(t.path(), &["--style", style]);
        assert!(out.status.success(), "{style} stderr: {}", stderr(&out));
        let text = stdout(&out);
        let labels: &[&str] = if style == "workwear" {
            &["BRANCH:", "FILES:", "ATTENTION:"]
        } else {
            &["Branch", "Files", "Attention"]
        };
        for needle in labels {
            assert!(
                text.contains(needle),
                "{style}: missing {needle:?} in:\n{text}"
            );
        }
        for needle in [root.as_str(), "main", "modified"] {
            assert!(
                text.contains(needle),
                "{style}: missing {needle:?} in:\n{text}"
            );
        }
    }
}

/// 6. Presets differ visually (human target, color forced) without semantic change.
#[test]
fn presets_differ_visually() {
    let t = plain_dir();
    let snap = scan_snapshot(t.path());
    let mut rendered = Vec::new();
    for preset in [
        Preset::House,
        Preset::BlackTie,
        Preset::Workwear,
        Preset::Studio,
    ] {
        let ctx = RenderContext::detect()
            .with_config(
                Config::default()
                    .with_preset(preset)
                    .with_color(sartorial::ColorChoice::Always),
            )
            .with_target(RenderTarget::Human);
        let screen = present::summary_screen(&snap);
        let mut buf = Vec::new();
        sartorial::RenderHuman::render_human(&screen, &ctx, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(
            text.contains('\x1b'),
            "{preset:?} should style human output"
        );
        assert!(
            text.contains("Files") || text.contains("FILES:"),
            "{preset:?} lost semantic content"
        );
        rendered.push(text);
    }
    for i in 0..rendered.len() {
        for j in (i + 1)..rendered.len() {
            assert_ne!(
                rendered[i], rendered[j],
                "presets {i} and {j} look identical"
            );
        }
    }
}

/// 7. Partial snapshots map to Attention without altering the snapshot.
#[test]
fn partial_maps_to_attention_presentation() {
    let t = plain_dir();
    let mut snap = scan_snapshot(t.path());
    assert_eq!(present::completion_status(&snap), Status::Ready);
    snap.partial = true;
    snap.suppressed_warnings = 2;
    snap.warnings.push(repocard::ScanWarning {
        category: "git".to_string(),
        path: None,
        message: "git unavailable (no git); continuing".to_string(),
    });
    let before = snap.clone();
    assert_eq!(present::completion_status(&snap), Status::Attention);
    let screen = present::summary_screen(&snap);
    assert_eq!(screen.status, Status::Attention);
    assert!(
        screen
            .notices
            .iter()
            .any(|n| n.message.starts_with("Partial scan")),
        "partial notice missing: {:?}",
        screen.notices
    );
    assert_eq!(snap, before, "adapter must not mutate the snapshot");
}

/// 8. Clean/dirty Git never invents a health judgement.
#[test]
fn dirty_git_is_not_a_health_verdict() {
    for dirty in [false, true] {
        let t = git_repo(dirty);
        let snap = scan_snapshot(t.path());
        assert!(!snap.partial);
        assert_eq!(present::completion_status(&snap), Status::Ready);
        let out = run(t.path(), &[]);
        let text = stdout(&out).to_lowercase();
        for bad in [
            "unhealthy",
            "broken",
            "health score",
            "health:",
            " bad",
            "failing",
        ] {
            assert!(!text.contains(bad), "{bad:?} judgement leaked in:\n{text}");
        }
    }
    let t = git_repo(true);
    let text = stdout(&run(t.path(), &[]));
    assert!(
        text.contains("modified"),
        "dirty state must still be factual"
    );
}

/// 8b. Conflicts are surfaced as an explicit fact, still Ready when complete.
#[test]
fn conflicts_surfaced_calmly() {
    let t = git_repo(false);
    let mut snap = scan_snapshot(t.path());
    snap.git.as_mut().unwrap().clean = false;
    snap.git.as_mut().unwrap().conflict_paths = vec!["a.txt".to_string()];
    assert_eq!(present::completion_status(&snap), Status::Ready);
    let screen = present::summary_screen(&snap);
    assert_eq!(screen.status, Status::Ready);
    assert!(
        screen
            .notices
            .iter()
            .any(|n| n.message.contains("Merge conflicts present")),
        "conflict notice missing"
    );
}

/// 9. Scan progress never fabricates percentages for unknown work.
#[test]
fn progress_has_no_fake_percentage() {
    use repocard::ScanPhase;
    let mut bar = present::scan_progress("demo-root");
    for phase in [
        ScanPhase::Discover,
        ScanPhase::Git,
        ScanPhase::Walk,
        ScanPhase::Languages,
        ScanPhase::Attention,
        ScanPhase::History,
        ScanPhase::Project,
        ScanPhase::Finalize,
    ] {
        bar.update_subtask(present::phase_label(phase));
        let st = bar.state();
        assert_eq!(st.mode, sartorial::ProgressMode::Activity);
        assert!(st.current.is_none() && st.total.is_none() && st.percent.is_none());
        assert!(st.derived_percent().is_none());
    }
}

/// 10. Non-TTY progress is bounded static output mentioning the scan root.
#[test]
fn non_tty_progress_is_bounded() {
    let t = plain_dir();
    let out = run(t.path(), &[]);
    let err = stderr(&out);
    let lines: Vec<&str> = err.lines().collect();
    assert!(lines.len() <= 4, "progress spam: {err:?}");
    assert!(err.contains("Scanning"), "root must be named: {err:?}");
    assert!(!err.contains('%'), "no percentages: {err:?}");
}

/// 11. `--motion never` suppresses animation.
#[test]
fn motion_never_suppresses_animation() {
    let ctx = RenderContext::detect().with_config(
        Config::default()
            .with_preset(Preset::House)
            .with_motion(MotionMode::Never),
    );
    assert!(!ctx.should_animate(true));
    let mut bar = present::scan_progress("demo-root");
    bar.start_live_with_tty(&ctx, false).unwrap();
    assert!(!bar.is_animating());
    let auto = RenderContext::detect().with_config(
        Config::default()
            .with_preset(Preset::House)
            .with_motion(MotionMode::Auto),
    );
    assert!(auto.should_animate(true), "auto must allow TTY animation");
    let t = plain_dir();
    let out = run(t.path(), &["--motion", "never"]);
    assert!(out.status.success());
    assert!(!stderr(&out).contains('\x1b'), "motion never: no ANSI");
}

/// 12. `write --dry-run` renders a Plan and writes nothing.
#[test]
fn dry_run_renders_plan_and_writes_nothing() {
    let t = plain_dir();
    // The binary canonicalizes the root before planning and strips the
    // `\\?\` prefix for display; mirror exactly that for the comparison.
    let canon = std::fs::canonicalize(t.path()).unwrap();
    let dest = canon.join(".repocard").join("report.json");
    let dest_shown = repocard::display_root(&dest);
    let out = Command::new(bin())
        .arg("write")
        .arg(t.path())
        .arg("--dry-run")
        .output()
        .expect("spawn repocard");
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Dry run"), "plan marker missing:\n{text}");
    assert!(
        text.contains(dest_shown.as_str()),
        "destination missing:\n{text}"
    );
    assert!(!dest.exists(), "dry-run must not write");
}

/// 13. Successful human write renders a Receipt from the actual result.
#[test]
fn write_receipt_reflects_actual_result() {
    let t = plain_dir();
    let canon = std::fs::canonicalize(t.path()).unwrap();
    let dest = canon.join(".repocard").join("report.json");
    let dest_shown = repocard::display_root(&dest);
    let out = Command::new(bin())
        .arg("write")
        .arg(t.path())
        .output()
        .expect("spawn repocard");
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains(dest_shown.as_str()),
        "receipt destination missing:\n{text}"
    );
    let raw = fs::read_to_string(&dest).expect("report written");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["schema_version"], "repocard.v0.1");
    let bytes = fs::metadata(&dest).unwrap().len();
    assert!(
        text.contains(&bytes.to_string()),
        "byte count missing:\n{text}"
    );
}

/// 15. Windows paths with spaces and Unicode render sensibly.
#[test]
fn spaced_unicode_paths_render() {
    let base = TempDir::new().unwrap();
    let dir = base.path().join("my repo ünïcode");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("héllo.rs"), "fn main() {}\n").unwrap();
    let out = run(&dir, &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("my repo"), "spaced path missing:\n{text}");
    let jout = run(&dir, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&jout)).unwrap();
    assert!(v["repository"]["root"]
        .as_str()
        .unwrap()
        .contains("my repo"));
}

/// 16. Machine JSON is identical across presentation flags on one fixture.
#[test]
fn json_identical_across_styles() {
    let t = plain_dir();
    let mut bodies = Vec::new();
    for args in [
        vec!["--json"],
        vec!["--json", "--style", "house"],
        vec!["--json", "--style", "black-tie"],
        vec!["--json", "--style", "workwear"],
        vec!["--json", "--style", "studio"],
    ] {
        let out = run(t.path(), &args);
        assert!(out.status.success(), "{args:?} stderr: {}", stderr(&out));
        assert!(
            stderr(&out).is_empty(),
            "{args:?} JSON stderr must be silent"
        );
        let text = stdout(&out);
        assert!(
            !text.contains('\x1b'),
            "{args:?} JSON must not contain ANSI"
        );
        let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(
            v["schema_version"], "repocard.v0.1",
            "{args:?} schema drift"
        );
        bodies.push(text);
    }
    for (i, body) in bodies.iter().enumerate().skip(1) {
        assert_eq!(*body, bodies[0], "JSON differs for style combo {i}");
    }
}

fn structural_ctx(preset: Preset) -> RenderContext {
    // Human target, colour off, full glyphs, fixed width: differences left
    // standing are structural, never ANSI.
    RenderContext::detect()
        .with_config(
            Config::default()
                .with_preset(preset)
                .with_color(ColorChoice::Never)
                .with_symbols(SymbolMode::Unicode)
                .with_width(100),
        )
        .with_target(RenderTarget::Human)
}

fn render_structural(preset: Preset, snap: &RepoSnapshot) -> String {
    let screen = present::summary_screen(snap);
    let mut buf = Vec::new();
    sartorial::RenderHuman::render_human(&screen, &structural_ctx(preset), &mut buf).unwrap();
    String::from_utf8(buf).unwrap()
}

/// 17. v0.2 preset grammars survive the adapter as structure, not colour.
#[test]
fn preset_grammars_are_structural() {
    let t = plain_dir();
    let snap = scan_snapshot(t.path());
    let house = render_structural(Preset::House, &snap);
    let black_tie = render_structural(Preset::BlackTie, &snap);
    let workwear = render_structural(Preset::Workwear, &snap);
    let studio = render_structural(Preset::Studio, &snap);

    // Same facts everywhere (values never change voice).
    for (name, out) in [
        ("house", &house),
        ("black-tie", &black_tie),
        ("workwear", &workwear),
        ("studio", &studio),
    ] {
        for needle in ["TODO 1", "FIXME 1", "2 files", "READY"] {
            assert!(out.contains(needle), "{name}: lost fact {needle:?}:\n{out}");
        }
    }

    // HOUSE: uppercase title, inline bounded status, ordinary fact labels,
    // no operator marker.
    let first = house.lines().next().unwrap_or("");
    assert!(
        first.starts_with("REPOCARD:"),
        "house title casing:\n{house}"
    );
    assert!(
        house
            .lines()
            .any(|l| l.contains("Status") && l.contains("READY")),
        "house inline status:\n{house}"
    );
    assert!(house.contains("Root"), "house ordinary labels:\n{house}");
    assert!(
        !house
            .lines()
            .any(|l| l.starts_with("» ") || l.starts_with("> ")),
        "house must not use the operator marker:\n{house}"
    );

    // BLACK TIE: preserved-case title, restrained title rule, stacked status.
    let first = black_tie.lines().next().unwrap_or("");
    assert!(
        first.starts_with("RepoCard:"),
        "black-tie title preserves case:\n{black_tie}"
    );
    assert!(
        black_tie
            .lines()
            .take(4)
            .any(|l| !l.is_empty() && l.chars().all(|c| c == '─')),
        "black-tie title rule:\n{black_tie}"
    );
    assert!(
        black_tie.lines().any(|l| l.trim() == "Status"),
        "black-tie stacked status:\n{black_tie}"
    );

    // WORKWEAR: operator marker, uppercase colon labels, tightest output.
    let first = workwear.lines().next().unwrap_or("");
    assert!(
        first.starts_with("» REPOCARD:"),
        "workwear operator title:\n{workwear}"
    );
    for needle in ["ROOT:", "FILES:", "ATTENTION:"] {
        assert!(
            workwear.contains(needle),
            "workwear {needle:?}:\n{workwear}"
        );
    }
    let blanks = |s: &str| s.lines().filter(|l| l.trim().is_empty()).count();
    assert!(
        blanks(&workwear) < blanks(&house),
        "workwear must be tighter than house"
    );

    // STUDIO: preserved-case title, stacked status, clearly more air.
    let first = studio.lines().next().unwrap_or("");
    assert!(
        first.starts_with("RepoCard:"),
        "studio title preserves case:\n{studio}"
    );
    assert!(
        studio.lines().any(|l| l.trim() == "Status"),
        "studio stacked status:\n{studio}"
    );
    assert!(
        studio.lines().count() > house.lines().count(),
        "studio must carry more vertical air than house"
    );

    // All four silhouettes differ with colour disabled.
    let all = [&house, &black_tie, &workwear, &studio];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(all[i], all[j], "silhouettes {i} and {j} identical");
        }
    }
}

/// 18. Workwear plain output stays boring, readable and pipe-safe.
#[test]
fn workwear_plain_is_pipe_safe() {
    let t = git_repo(false);
    let out = run(t.path(), &["--plain", "--style", "workwear"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(!text.contains('\x1b'), "plain must not contain ANSI");
    // No Unicode *structural* leakage: framework markers, badges, rules and
    // key glyphs must all be ASCII. (Fact *values* such as the middle dot in
    // "N files · M bytes" are RepoCard content, not framework grammar, and
    // are unchanged by this adoption.)
    for glyph in ['»', '✓', '×', '●', '○', '–', '─', '↑', '↓', '←', '→'] {
        assert!(!text.contains(glyph), "plain leaked {glyph:?}:\n{text}");
    }
    assert!(text.contains("> "), "ASCII operator marker:\n{text}");
    for needle in ["BRANCH:", "FILES:", "main"] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

/// 19. The binary reports the Cargo package version (version truth).
#[test]
fn version_reports_cargo_package_version() {
    let out = Command::new(bin())
        .arg("--version")
        .output()
        .expect("spawn repocard");
    assert!(out.status.success());
    let text = stdout(&out);
    assert_eq!(
        text.trim(),
        format!("repocard {}", env!("CARGO_PKG_VERSION")),
        "binary must report its Cargo package version"
    );
}
