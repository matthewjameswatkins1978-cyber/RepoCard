# RepoCard v0.1 — Small, Fast Repository Intelligence Card

RepoCard looks at a software repository or ordinary directory and produces one
compact, bounded, deterministic snapshot of its current state.

```
repocard [PATH] [--json] [--details]
repocard write [PATH] [--output <PATH>] [--dry-run] [--force]
```

## What it does

One operation answers:

- Where am I? (identity, root, git or plain directory)
- Git state: branch, commit, dirty/conflicted, ahead/behind (local knowledge only), stash
- Languages (code-line percentages via Tokei), file/test stats
- TODO / FIXME / HACK / XXX markers (bounded, no source snippets stored)
- Largest files, hot files (bounded 200-commit window)
- CI providers, manifests/toolchains, README/licence, AI/agent guidance files
- Warnings + `partial` flag when information could not be collected

## Examples

```
repocard .
repocard . --details
repocard . --json
repocard write . --dry-run
repocard write . --output C:\temp\card.json --force
```

`--json` emits ONLY JSON to stdout. Warnings live inside the snapshot.
Default report: `.repocard/report.json` (refuses overwrite without `--force`).

## Deliberately not

Not a linter, reviewer, scorer, package manager, Git replacement, or GitHub
client. No health score. No AI summaries. No network requests. Default
operation is read-only (`repocard write` is the only mutating command).

## Facts and meanings

- **Hot files**: paths appearing most often as changed within the sampled
  history window (default 200 commits, top 10). Churn signal only — not a
  quality judgement.
- **Test footprint**: file-level heuristic (`tests/`, `*_test.go`,
  `test_*.py`, `*.test.*`, `*.spec.*`, …). It counts *test files*, not tests.
- **Ahead/behind**: reflects local Git knowledge only. RepoCard never fetches.
- **JSON schema version**: `repocard.v0.1`.
- **Report write**: plan → serialize → create `.repocard/` → atomic-ish
  temp+rename → receipt. `--dry-run` prints the plan and writes nothing.

## Offline / Git-optional

Works on ordinary directories. If Git is absent or the directory is not a
repository, Git state is `null` plus a factual warning; filesystem, language,
and project facts are still reported.

## Sartorial

`RepoSnapshot` is the semantic authority; the scanner knows nothing about
ANSI, colours, or layout. `ScanEvent` phases (Discover/Git/Walk/Languages/
Attention/History/Project/Finalize) are the future motion boundary. Adding
Sartorial later should be `RepoSnapshot -> components` with no scanner
rewrites.
