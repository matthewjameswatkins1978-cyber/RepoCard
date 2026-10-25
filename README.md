# RepoCard

**Know the shape of a repository in one command.**

RepoCard turns a software checkout, or even an ordinary directory, into a compact, deterministic snapshot: what it is, what tools it uses, what Git knows locally, and where attention may be useful.

It's a Rust CLI for developers and coding agents who need quick orientation **without an AI summary, an index server or a network call**.

**Current implementation:** 0.2.0. The machine-readable snapshot contract is still named `repocard.v0.1`; product version and data schema version are separate.

## See it work

```sh
repocard .
repocard . --details
repocard . --json
repocard . --plain
repocard . --style workwear --motion never
```

The short human display is designed to be readable at a glance. `--details` exposes more context. `--json` emits the snapshot alone for programs and agents, without terminal decoration.

Want a saved report?

```sh
repocard write . --dry-run
repocard write .
repocard write . --output ./repository-card.json --force
```

A normal scan is read-only. `write` is the explicit mutating command; its default destination is `.repocard/report.json`, and existing reports aren't overwritten without `--force`.

## What goes on the card?

- Git branch, current commit, local dirty/conflict state, stash and locally known ahead/behind information.
- Languages and approximate code-line proportions (via Tokei).
- Bounded test-file counts, TODO/FIXME/HACK/XXX markers and largest files.
- Frequent-change paths from a bounded local commit history.
- Project manifests, toolchains, CI hints, README/licence presence and agent guidance files.
- Warnings and a `partial` flag when a fact couldn't be collected.

**Evidence, not grading.** “Hot files” only means frequently changed files in the sampled history; it isn't a code-quality score. Test footprints count likely test *files*, not test cases. Local Git state is not a substitute for a remote fetch.

## Install from source

Requires a stable Rust toolchain and Git to clone the repository.

```sh
git clone https://github.com/matthewjameswatkins1978-cyber/RepoCard.git
cd RepoCard
cargo install --path .
repocard --help
```

The application uses [Sartorial](https://github.com/matthewjameswatkins1978-cyber/Sartorial) for terminal presentation. Choose House (default), Black Tie, Workwear or Studio with `--style`; use `--plain` for pipe-safe text and `--motion never` to suppress live progress.

## Deliberately small

RepoCard does **not** lint or review code, contact GitHub, run an LLM, score repository health or rewrite your files. It also works when Git is unavailable: Git fields are absent and the snapshot explains what couldn't be inspected.

Think of it as a repository's calling card, not its final examination.
