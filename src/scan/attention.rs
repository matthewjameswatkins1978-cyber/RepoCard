use crate::model::{AttentionLocation, AttentionSnapshot, WarningSink};
use crate::scan::files::WalkedFile;
use aho_corasick::AhoCorasick;
use std::fs::File;
use std::io::Read;

const PATTERNS: [&str; 4] = ["TODO", "FIXME", "HACK", "XXX"];

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn scan_attention(
    walked: &[WalkedFile],
    content_max_bytes: u64,
    locations_limit: usize,
    warnings: &mut WarningSink,
) -> AttentionSnapshot {
    let ac = AhoCorasick::new(PATTERNS).expect("valid patterns");
    let mut todo = 0u64;
    let mut fixme = 0u64;
    let mut hack = 0u64;
    let mut xxx = 0u64;
    let mut locations: Vec<AttentionLocation> = Vec::new();
    let mut scanned_text = 0u64;
    let mut skipped_large = 0u64;
    let mut truncated = false;

    for f in walked {
        if f.bytes > content_max_bytes {
            // Only count as skipped-large if it looks like text? Spec says
            // "If a text file is larger" skip. We avoid reading the whole file;
            // peek first chunk for binary check, then count.
            match peek_is_text(&f.abs_path) {
                Some(true) => skipped_large += 1,
                Some(false) => {}
                None => {
                    warnings.push(
                        "attention",
                        Some(f.rel_path.clone()),
                        "cannot read file for marker scan".to_string(),
                    );
                }
            }
            continue;
        }
        let bytes = match read_bounded(&f.abs_path, content_max_bytes) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(
                    "attention",
                    Some(f.rel_path.clone()),
                    format!("cannot read file: {e}"),
                );
                continue;
            }
        };
        if content_inspector::inspect(&bytes).is_binary() {
            continue;
        }
        scanned_text += 1;
        // ASCII word-boundary aware counting, line numbers via newline scan.
        // Build line-start index.
        let mut line_starts: Vec<usize> = vec![0];
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        let line_of = |offset: usize| -> u64 {
            match line_starts.binary_search(&offset) {
                Ok(n) => (n as u64) + 1,
                Err(n) => n as u64,
            }
        };
        for mat in ac.find_iter(&bytes) {
            let s = mat.start();
            let e = mat.end();
            // Word boundary: char before/after must not be [A-Za-z0-9_].
            let before_ok = s == 0 || !is_word_char(bytes[s - 1]);
            let after_ok = e >= bytes.len() || !is_word_char(bytes[e]);
            if !(before_ok && after_ok) {
                continue;
            }
            let line = line_of(s);
            match mat.pattern().as_usize() {
                0 => todo += 1,
                1 => fixme += 1,
                2 => hack += 1,
                3 => xxx += 1,
                _ => {}
            }
            if locations.len() < locations_limit {
                locations.push(AttentionLocation {
                    marker: PATTERNS[mat.pattern().as_usize()].to_string(),
                    path: f.rel_path.clone(),
                    line,
                });
            } else {
                truncated = true;
            }
        }
    }

    locations.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.marker.cmp(&b.marker))
    });

    AttentionSnapshot {
        todo_count: todo,
        fixme_count: fixme,
        hack_count: hack,
        xxx_count: xxx,
        locations,
        truncated,
        scanned_text_files: scanned_text,
        skipped_large_text_files: skipped_large,
    }
}

fn read_bounded(path: &std::path::Path, max: u64) -> std::io::Result<Vec<u8>> {
    let f = File::open(path)?;
    let mut buf = Vec::new();
    // max + 1 to detect overflow (shouldn't happen since caller checks size,
    // but size may change between walk and read).
    let limit = max.saturating_add(1) as usize;
    f.take(limit as u64).read_to_end(&mut buf)?;
    if buf.len() > max as usize {
        buf.truncate(max as usize);
    }
    Ok(buf)
}

fn peek_is_text(path: &std::path::Path) -> Option<bool> {
    let mut f = File::open(path).ok()?;
    let mut buf = [0u8; 8192];
    let n = f.read(&mut buf).ok()?;
    let slice = &buf[..n];
    if slice.is_empty() {
        return Some(true);
    }
    Some(content_inspector::inspect(slice).is_text())
}
