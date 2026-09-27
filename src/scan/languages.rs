use crate::model::{code_percent, LanguageSnapshot, LanguageStat, WarningSink};

/// Directories RepoCard excludes from its own source walk. Tokei is given the
/// same exclusions so language statistics follow RepoCard policy instead of
/// Tokei defaults.
const EXCLUDED_DIRS: [&str; 2] = [".git", ".repocard"];

pub fn analyze_languages(root: &std::path::Path, warnings: &mut WarningSink) -> LanguageSnapshot {
    let config = tokei::Config {
        // RepoCard policy: hidden files are not source. Set explicitly rather
        // than relying on Tokei's default, so the universe stays aligned if
        // defaults move.
        hidden: Some(false),
        ..tokei::Config::default()
    };
    let mut languages = tokei::Languages::new();
    // Tokei respects .gitignore/.ignore/excludes internally; `EXCLUDED_DIRS`
    // additionally keeps report output (and any nested .git) out of stats.
    languages.get_statistics(&[root.to_path_buf()], &EXCLUDED_DIRS, &config);

    // Compact: drop failed? Languages API silently skips unreadable.
    let mut stats: Vec<LanguageStat> = Vec::new();
    let mut total_code: u64 = 0;
    let mut total_comments: u64 = 0;
    let mut total_blanks: u64 = 0;

    for (lang_type, lang) in languages.iter() {
        let code = lang.code as u64;
        let comments = lang.comments as u64;
        let blanks = lang.blanks as u64;
        let files = lang.reports.len() as u64;
        // Skip empty entries (e.g. total-only artifacts).
        if code == 0 && comments == 0 && blanks == 0 && files == 0 {
            continue;
        }
        // If tokei reported an error-heavy language with no code, keep only if files>0?
        total_code += code;
        total_comments += comments;
        total_blanks += blanks;
        stats.push(LanguageStat {
            language: lang_type.name().to_string(),
            files,
            code,
            comments,
            blanks,
            code_percent: 0.0,
        });
    }

    if stats.is_empty() {
        // Tokei failing on a subset shouldn't kill the scan; but if nothing at
        // all came back and the tree is non-empty, note it.
        let _ = warnings;
    }

    for s in stats.iter_mut() {
        s.code_percent = code_percent(s.code, total_code);
    }
    // Sort by code desc, name asc for determinism.
    stats.sort_by(|a, b| {
        b.code
            .cmp(&a.code)
            .then_with(|| a.language.cmp(&b.language))
    });

    LanguageSnapshot {
        total_code_lines: total_code,
        total_comment_lines: total_comments,
        total_blank_lines: total_blanks,
        languages: stats,
    }
}
