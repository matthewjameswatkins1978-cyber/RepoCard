use repocard::{scan, ScanOptions};
use std::fs;
use tempfile::TempDir;

#[test]
fn project_metadata_detected() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("README.md"), "# hi").unwrap();
    fs::write(t.path().join("LICENSE"), "MIT").unwrap();
    fs::create_dir_all(t.path().join(".github/workflows")).unwrap();
    fs::write(t.path().join(".github/workflows/ci.yml"), "on: push").unwrap();
    fs::write(t.path().join(".gitlab-ci.yml"), "stages: []").unwrap();
    fs::write(t.path().join("Cargo.toml"), "[package]\nname=\"x\"").unwrap();
    fs::write(t.path().join("package.json"), "{}").unwrap();
    fs::write(t.path().join("pyproject.toml"), "[project]").unwrap();
    fs::write(t.path().join("AGENTS.md"), "# agents").unwrap();
    fs::create_dir_all(t.path().join(".github")).unwrap();
    fs::write(
        t.path().join(".github/copilot-instructions.md"),
        "# copilot",
    )
    .unwrap();

    let s = scan(t.path(), &ScanOptions::default(), |_| {}).unwrap();
    assert_eq!(s.project.readme.as_deref(), Some("README.md"));
    assert_eq!(s.project.licence.as_deref(), Some("LICENSE"));
    assert!(s.project.ci.contains(&"github_actions".to_string()));
    assert!(s.project.ci.contains(&"gitlab_ci".to_string()));
    assert!(s.project.manifests.contains(&"Cargo.toml".to_string()));
    assert!(s.project.manifests.contains(&"package.json".to_string()));
    assert!(s.project.manifests.contains(&"pyproject.toml".to_string()));
    assert!(s.project.guidance.contains(&"AGENTS.md".to_string()));
    assert!(s
        .project
        .guidance
        .contains(&".github/copilot-instructions.md".to_string()));
}
