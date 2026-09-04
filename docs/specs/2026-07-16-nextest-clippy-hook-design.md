# Nextest CI and edit-time Clippy design

## Goal

Run Rust tests in CI with per-test process isolation, and report Clippy
failures immediately after Claude Code edits a Rust file without making the
hook unnecessarily slow.

## CI design

Install a prebuilt `cargo-nextest` with
`taiki-e/install-action@nextest`, following nextest's recommended GitHub
Actions setup and the repository's existing use of tool-specific installer
tags. Replace the existing unit-test command with
`cargo nextest run --release`.

Nextest does not run doctests, but JSSE currently has no library target and
`cargo test --doc --release` reports that there are no library targets. The
replacement therefore does not remove existing doctest coverage. If a library
target is introduced later, CI should add a separate doctest command.

Building nextest from source with `cargo install --locked` was considered, but
would add avoidable compile time to every clean CI runner. Downloading an
archive directly was also considered, but the installer action provides a
clearer, maintained integration.

## PostToolUse hook design

Keep `scripts/fmt-hook.sh` as the configured hook entry point to avoid a
settings migration. For an existing `.rs` file:

1. Format only the edited file with `cargo fmt`.
2. Map the file to its Cargo target:
   - `src/main.rs`, modules below `src/`, and `build.rs` use `--all-targets`;
   - a future `src/lib.rs` uses `--lib`;
   - `tests/<name>.rs`, `examples/<name>.rs`, and `benches/<name>.rs` use the
     corresponding named target.
3. Run Clippy with `-D warnings` for only that target.
4. Exit with status 2 and write diagnostics to stderr when formatting or
   Clippy fails, which returns actionable PostToolUse feedback to Claude.

Cargo and Clippy compile crates rather than arbitrary source files, so the
containing target is the narrowest reliable scope. A bare `--bin jsse` was
considered for `src/` modules but rejected: JSSE is a binary-only crate, so that
compiles only the non-test build and silently skips `#[cfg(test)]` source
(inline test modules and files such as `src/interpreter/tests.rs`), reporting
success without ever linting the edit. `--all-targets` also compiles the
cfg(test) build, and on this crate (no `benches/` or `examples/`) it resolves to
just the binary plus its test build and the single integration test. Filtering
full-crate JSON diagnostics to the edited file was considered, but could hide
errors caused by the edit at another source location while retaining the same
compile cost.

Rust files outside known Cargo targets are formatted but do not trigger
Clippy. Non-Rust files remain a no-op.

## Verification

- Run the existing release tests and compare them with
  `cargo nextest run --release` to detect reliance on shared in-process state.
- Test hook no-op behavior, formatting, target selection, and lint-failure
  feedback with temporary fixtures or command wrappers.
- Measure a warm hook invocation to confirm incremental Clippy latency is
  suitable for edit-time use.
- Run the repository's full local quality gate before opening the pull
  request.
