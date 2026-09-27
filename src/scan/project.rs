use crate::model::ProjectSnapshot;
use std::path::Path;

const READMES: [&str; 4] = ["README", "README.md", "README.rst", "README.txt"];
const LICENCES: [&str; 5] = ["LICENSE", "LICENSE.md", "LICENCE", "LICENCE.md", "COPYING"];

const MANIFESTS: [&str; 23] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rust-toolchain",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "pyproject.toml",
    "requirements.txt",
    "uv.lock",
    "poetry.lock",
    "go.mod",
    "go.sum",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "gradlew",
    "Gemfile",
    "composer.json",
    "mix.exs",
];

const GUIDANCE: [&str; 6] = [
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".github/copilot-instructions.md",
    ".cursorrules",
    ".cursor/rules",
];

pub fn detect_project(root: &Path) -> ProjectSnapshot {
    let readme = READMES
        .iter()
        .find(|n| root.join(n).is_file())
        .map(|s| s.to_string());
    let licence = LICENCES
        .iter()
        .find(|n| root.join(n).is_file())
        .map(|s| s.to_string());

    let mut ci: Vec<String> = Vec::new();
    if has_glob_dir(root, ".github/workflows", &["yml", "yaml"]) {
        ci.push("github_actions".to_string());
    }
    if root.join(".gitlab-ci.yml").is_file() {
        ci.push("gitlab_ci".to_string());
    }
    if root.join(".circleci/config.yml").is_file() {
        ci.push("circleci".to_string());
    }
    if root.join("azure-pipelines.yml").is_file() {
        ci.push("azure_pipelines".to_string());
    }
    if root.join("Jenkinsfile").is_file() {
        ci.push("jenkins".to_string());
    }
    if root.join(".buildkite").exists() {
        ci.push("buildkite".to_string());
    }
    if root.join("bitbucket-pipelines.yml").is_file() {
        ci.push("bitbucket".to_string());
    }
    ci.sort();
    ci.dedup();

    let mut manifests: Vec<String> = Vec::new();
    for m in MANIFESTS {
        if root.join(m).is_file() {
            manifests.push(m.to_string());
        }
    }
    // .NET globs: *.sln, *.csproj at root.
    if let Ok(rd) = std::fs::read_dir(root) {
        for ent in rd.flatten() {
            if let Some(name) = ent.file_name().to_str().map(|s| s.to_string()) {
                let lower = name.to_ascii_lowercase();
                if (lower.ends_with(".sln") || lower.ends_with(".csproj"))
                    && !manifests.contains(&name)
                {
                    manifests.push(name);
                }
            }
        }
    }
    manifests.sort();

    let mut guidance: Vec<String> = Vec::new();
    for g in GUIDANCE {
        if root.join(g).exists() {
            guidance.push(g.to_string());
        }
    }
    guidance.sort();

    ProjectSnapshot {
        readme,
        licence,
        ci,
        manifests,
        guidance,
    }
}

fn has_glob_dir(root: &Path, dir: &str, exts: &[&str]) -> bool {
    let d = root.join(dir);
    let rd = match std::fs::read_dir(&d) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for ent in rd.flatten() {
        if let Some(name) = ent.file_name().to_str().map(|s| s.to_ascii_lowercase()) {
            for ext in exts {
                if name.ends_with(&format!(".{ext}")) {
                    return true;
                }
            }
        }
    }
    false
}
