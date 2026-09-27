use repocard::{scan, ScanOptions};
use std::fs;
use tempfile::TempDir;

#[test]
fn languages_detected_for_rust_python_js() {
    let t = TempDir::new().unwrap();
    fs::write(
        t.path().join("main.rs"),
        "fn main() {\n    println!(\"hi\");\n}\n",
    )
    .unwrap();
    fs::write(t.path().join("app.py"), "def f():\n    return 1\n").unwrap();
    fs::write(t.path().join("app.js"), "function f() { return 1; }\n").unwrap();
    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    let names: Vec<&str> = s
        .languages
        .languages
        .iter()
        .map(|l| l.language.as_str())
        .collect();
    assert!(names.contains(&"Rust"), "missing Rust: {names:?}");
    assert!(names.contains(&"Python"), "missing Python: {names:?}");
    assert!(
        names.contains(&"JavaScript") || names.contains(&"TypeScript"),
        "missing JS: {names:?}"
    );
    assert!(s.languages.total_code_lines > 0);
    // Percentages are code-line based and sum to ~100.
    let sum: f64 = s.languages.languages.iter().map(|l| l.code_percent).sum();
    assert!(
        (sum - 100.0).abs() < 2.0 || sum == 0.0,
        "percentages should sum ~100, got {sum}"
    );
}
