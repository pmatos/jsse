# Pre-commit quality checks design

## Goal

Give every contributor the same fast, repository-wide checks before a commit
is created, catching common file hygiene, Python quality, spelling, and
documentation problems earlier than CI.

## Constraints

- Keep the hook configuration independent of npm: JSSE is a Rust project whose
  helper tooling is written in Python.
- Pin every third-party hook so installations are reproducible.
- Keep expensive Rust builds, Clippy, release tests, and test262 runs in the
  existing edit-time hook and CI rather than adding substantial latency to
  every commit.
- Establish a clean all-files baseline so a fresh checkout can run
  `pre-commit run --all-files` successfully.

## Approaches considered

1. Add only the generic `pre-commit-hooks` checks. This has the smallest rollout
   cost but leaves the repository's Python tooling and prose unchecked.
2. Add local hooks that run `./scripts/lint.sh` and the release test suite.
   This reuses existing commands, but Clippy and tests make the commit loop too
   slow and duplicate CI and the Claude edit-time hook.
3. Adopt the issue's complete pinned hook set and make the current tree pass it.
   This adds broad coverage while keeping hook runtimes focused on changed
   files. This is the selected approach.

## Design

Add `.pre-commit-config.yaml` with four pinned sources:

- `pre-commit-hooks` for large files, path conflicts, merge markers, broken
  symlinks, YAML/TOML syntax, debug statements, private keys, line endings,
  test naming, trailing whitespace, and final newlines;
- Ruff for automatic Python lint fixes and formatting;
- typos, in check-only mode, for spelling mistakes;
- pydoclint for Python docstring consistency.

Ruff's modifying hooks intentionally run before the non-modifying spelling and
docstring checks. Generated Unicode/emoji tables, their source cache, generated
test262 baselines, and the generated changelog are excluded from hook processing
because they contain machine-significant formatting, names, encodings, paths,
and hashes. `_typos.toml` records the small set of legitimate domain identifiers
and standardized names that resemble English misspellings, such as Unicode
general categories, currency codes, locale identifiers, and test-runner APIs.
Hook installation remains the standard `pre-commit install`; no project package
manifest is added solely to install developer tooling.

## Verification

- Validate the configuration with `pre-commit validate-config`.
- Run every hook against every tracked file with
  `pre-commit run --all-files`, applying and reviewing any mechanical cleanup.
- Run the repository's existing local quality gate before publishing.

This change affects development tooling only. It does not alter ECMAScript
syntax or runtime semantics, so the spec and targeted test262 areas do not
apply.
