use crate::model::RepoSnapshot;

pub fn render_compact(s: &RepoSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!("RepoCard: {}\n", s.repository.name));
    out.push_str(&format!("Root: {}\n", s.repository.root));
    out.push_str("\nGit:\n");
    match &s.git {
        Some(g) => {
            out.push_str(&format!(
                "  branch: {}\n",
                g.branch.clone().unwrap_or_else(|| "(detached)".to_string())
            ));
            out.push_str(&format!(
                "  head: {}\n",
                g.head_short.clone().unwrap_or_else(|| "-".to_string())
            ));
            out.push_str(&format!(
                "  upstream: {}\n",
                g.upstream.clone().unwrap_or_else(|| "-".to_string())
            ));
            out.push_str(&format!(
                "  ahead: {}\n",
                g.ahead
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ));
            out.push_str(&format!(
                "  behind: {}\n",
                g.behind
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ));
            out.push_str(&format!("  modified: {}\n", g.modified_paths.len()));
            out.push_str(&format!("  untracked: {}\n", g.untracked_paths.len()));
            out.push_str(&format!("  conflicts: {}\n", g.conflict_paths.len()));
            out.push_str(&format!(
                "  clean: {}\n",
                if g.clean { "yes" } else { "no" }
            ));
            if g.detached {
                out.push_str("  detached: yes\n");
            }
        }
        None => {
            out.push_str("  unavailable (not a git repository or git absent)\n");
        }
    }
    out.push_str("\nFiles:\n");
    out.push_str(&format!("  scanned: {}\n", s.filesystem.file_count));
    out.push_str(&format!("  bytes: {}\n", s.filesystem.scanned_bytes));
    out.push_str("\nLanguages:\n");
    if s.languages.languages.is_empty() {
        out.push_str("  (none detected)\n");
    } else {
        for l in s.languages.languages.iter().take(10) {
            out.push_str(&format!("  {}: {}\n", l.language, l.code));
        }
    }
    out.push_str("\nAttention:\n");
    out.push_str(&format!("  TODO: {}\n", s.attention.todo_count));
    out.push_str(&format!("  FIXME: {}\n", s.attention.fixme_count));
    out.push_str(&format!("  HACK: {}\n", s.attention.hack_count));
    out.push_str(&format!("  XXX: {}\n", s.attention.xxx_count));
    out.push_str("\nHot files:\n");
    if s.history.hot_files.is_empty() {
        out.push_str("  (no history)\n");
    } else {
        for h in &s.history.hot_files {
            out.push_str(&format!("  {}: {}\n", h.path, h.change_appearances));
        }
    }
    if s.partial {
        out.push_str(&format!(
            "\nWarnings: {} (suppressed: {})\n",
            s.warnings.len(),
            s.suppressed_warnings
        ));
    }
    out
}

pub fn render_details(s: &RepoSnapshot) -> String {
    let mut out = render_compact(s);
    out.push_str("\nTests:\n");
    out.push_str(&format!("  files: {}\n", s.tests.files.len()));
    for f in s.tests.files.iter().take(20) {
        out.push_str(&format!("  - {f}\n"));
    }
    if s.tests.files.len() > 20 {
        out.push_str(&format!("  ... ({} more)\n", s.tests.files.len() - 20));
    }
    out.push_str("\nLargest files:\n");
    for l in &s.filesystem.largest_files {
        out.push_str(&format!("  {}: {}\n", l.path, l.bytes));
    }
    out.push_str("\nAttention locations:\n");
    for l in &s.attention.locations {
        out.push_str(&format!("  {} {}:{}\n", l.marker, l.path, l.line));
    }
    if s.attention.truncated {
        out.push_str("  ... (truncated)\n");
    }
    out.push_str("\nProject:\n");
    out.push_str(&format!(
        "  readme: {}\n",
        s.project.readme.clone().unwrap_or_else(|| "-".to_string())
    ));
    out.push_str(&format!(
        "  licence: {}\n",
        s.project.licence.clone().unwrap_or_else(|| "-".to_string())
    ));
    out.push_str(&format!(
        "  ci: {}\n",
        if s.project.ci.is_empty() {
            "-".to_string()
        } else {
            s.project.ci.join(", ")
        }
    ));
    out.push_str(&format!(
        "  manifests: {}\n",
        if s.project.manifests.is_empty() {
            "-".to_string()
        } else {
            s.project.manifests.join(", ")
        }
    ));
    out.push_str(&format!(
        "  guidance: {}\n",
        if s.project.guidance.is_empty() {
            "-".to_string()
        } else {
            s.project.guidance.join(", ")
        }
    ));
    if !s.warnings.is_empty() {
        out.push_str("\nWarnings:\n");
        for w in &s.warnings {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                w.category,
                w.path.clone().unwrap_or_else(|| "-".to_string()),
                w.message
            ));
        }
        if s.suppressed_warnings > 0 {
            out.push_str(&format!("  ({} suppressed)\n", s.suppressed_warnings));
        }
    }
    out
}
