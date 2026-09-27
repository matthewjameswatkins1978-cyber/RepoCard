use repocard::{scan, ScanOptions};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_footprint_heuristic() {
    let t = TempDir::new().unwrap();
    fs::create_dir_all(t.path().join("tests")).unwrap();
    fs::create_dir_all(t.path().join("src")).unwrap();
    fs::write(t.path().join("tests/foo.rs"), "#[test] fn x() {}").unwrap();
    fs::write(t.path().join("test_example.py"), "def test_x(): pass").unwrap();
    fs::write(t.path().join("thing_test.go"), "package x").unwrap();
    fs::write(t.path().join("widget.spec.ts"), "describe('w')").unwrap();
    fs::write(t.path().join("src/main.rs"), "fn main() {}").unwrap();
    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    assert!(s.tests.heuristic);
    for expected in [
        "tests/foo.rs",
        "test_example.py",
        "thing_test.go",
        "widget.spec.ts",
    ] {
        assert!(
            s.tests.files.contains(&expected.to_string()),
            "missing {expected}: {:?}",
            s.tests.files
        );
    }
    assert!(!s.tests.files.contains(&"src/main.rs".to_string()));
}
