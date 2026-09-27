use repocard::{report, scan, ScanOptions};
use std::fs;
use tempfile::TempDir;

#[test]
fn json_is_valid_and_deterministic() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("ünicode file.txt"), "TODO hi\n").unwrap();
    let opts = ScanOptions::default();
    let s1 = scan(t.path(), &opts, |_| {}).unwrap();
    let j1 = serde_json::to_string_pretty(&s1).unwrap();
    let s2 = scan(t.path(), &opts, |_| {}).unwrap();
    let j2 = serde_json::to_string_pretty(&s2).unwrap();
    assert_eq!(j1, j2, "JSON must be deterministic");
    let v: serde_json::Value = serde_json::from_str(&j1).unwrap();
    assert_eq!(v["schema_version"], "repocard.v0.1");
    // No ANSI, no snippets.
    assert!(!j1.contains('\x1b'));
    assert!(!j1.contains("TODO hi"));
    // Unicode preserved.
    assert!(j1.contains("nicode"));
}

#[test]
fn report_write_dry_run_force_receipt() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("a.txt"), "hello").unwrap();
    let root = std::fs::canonicalize(t.path()).unwrap();
    let dest = root.join(".repocard").join("report.json");

    // Dry-run writes nothing.
    let plan = report::plan_report(&root, &dest);
    assert!(plan.will_create_directory);
    assert!(plan.will_create_file);
    assert!(!dest.exists());

    // Actual write creates dir + file with valid JSON.
    let snap = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    let receipt = report::write_report(&snap, &dest, false).unwrap();
    assert!(dest.exists());
    assert!(receipt.bytes_written > 0);
    assert_eq!(receipt.schema_version, "repocard.v0.1");
    let raw = fs::read_to_string(&dest).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["schema_version"], "repocard.v0.1");

    // Refuses overwrite without --force.
    let err = report::write_report(&snap, &dest, false).unwrap_err();
    assert!(err.to_string().contains("--force"));

    // --force replaces.
    let receipt2 = report::write_report(&snap, &dest, true).unwrap();
    assert!(receipt2.bytes_written > 0);
    // No corrupt half-written tmp left behind.
    assert!(!root.join(".repocard").join("report.json.tmp").exists());
}

#[test]
fn scan_events_cover_phases() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("a.txt"), "x").unwrap();
    let mut phases = Vec::new();
    scan(t.path(), &ScanOptions::default(), |e| phases.push(e.phase)).unwrap();
    for p in [
        repocard::ScanPhase::Discover,
        repocard::ScanPhase::Git,
        repocard::ScanPhase::Walk,
        repocard::ScanPhase::Languages,
        repocard::ScanPhase::Attention,
        repocard::ScanPhase::History,
        repocard::ScanPhase::Project,
        repocard::ScanPhase::Finalize,
    ] {
        assert!(phases.contains(&p), "missing phase {p:?} in {phases:?}");
    }
}
