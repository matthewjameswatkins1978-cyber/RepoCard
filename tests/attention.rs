use repocard::{scan, ScanOptions};
use std::fs;
use tempfile::TempDir;

#[test]
fn attention_counts_markers_with_lines_and_no_snippets() {
    let t = TempDir::new().unwrap();
    fs::write(
        t.path().join("a.rs"),
        "// TODO: fix this\nfn x() {} // FIXME now\n// HACK ok\n// XXX note\n// TODO again\n",
    )
    .unwrap();
    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    assert_eq!(s.attention.todo_count, 2);
    assert_eq!(s.attention.fixme_count, 1);
    assert_eq!(s.attention.hack_count, 1);
    assert_eq!(s.attention.xxx_count, 1);
    // Line numbers correct and sorted.
    let todo_lines: Vec<u64> = s
        .attention
        .locations
        .iter()
        .filter(|l| l.marker == "TODO")
        .map(|l| l.line)
        .collect();
    assert_eq!(todo_lines, vec![1, 5]);
    // No source snippets stored.
    let json = serde_json::to_value(&s.attention).unwrap().to_string();
    assert!(!json.contains("fix this"));
}

#[test]
fn attention_word_boundaries() {
    let t = TempDir::new().unwrap();
    // METHODODO should not count; standalone TODO should.
    fs::write(t.path().join("a.txt"), "METHODODO blah\nTODO\n").unwrap();
    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    assert_eq!(s.attention.todo_count, 1);
}

#[test]
fn attention_skips_binary_and_large_files() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("bin.dat"), vec![0u8, 159, 146, 0, 10, 20, 30]).unwrap();
    fs::write(t.path().join("ok.txt"), "TODO here\n").unwrap();
    let opts = ScanOptions {
        content_scan_max_bytes: 10,
        ..ScanOptions::default()
    };
    fs::write(t.path().join("large.txt"), "TODO ".repeat(100)).unwrap();
    let s = scan(t.path(), &opts, |_| {}).unwrap();
    // Binary skipped silently; large text counted as skipped.
    assert!(s.attention.skipped_large_text_files >= 1);
}

#[test]
fn attention_location_cap_keeps_counts() {
    let t = TempDir::new().unwrap();
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!("// TODO item {i}\n"));
    }
    fs::write(t.path().join("many.txt"), content).unwrap();
    let opts = ScanOptions {
        attention_locations_limit: 10,
        ..ScanOptions::default()
    };
    let s = scan(t.path(), &opts, |_| {}).unwrap();
    assert_eq!(s.attention.todo_count, 50);
    assert_eq!(s.attention.locations.len(), 10);
    assert!(s.attention.truncated);
}
