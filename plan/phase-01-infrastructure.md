# Phase 1: Project Scaffolding & Infrastructure

## Goal
Set up the Rust project, CLI interface, test262 runner, and CI pipeline.

## Tasks

### 1.1 Rust Project Setup
- [x] `cargo init` with binary crate
- [x] Set up workspace if needed (e.g., `jsse-core`, `jsse-cli`)
- [x] Choose Rust edition (2024)
- [x] Set up `Cargo.toml` with initial dependencies
- [x] Add `.gitignore` for Rust

### 1.2 CLI Interface
- [x] Accept JS file path as argument
- [x] Accept `--eval` / `-e` for inline JS
- [x] REPL mode when no arguments given
- [x] `--version` and `--help` flags
- [x] Exit codes: 0 for success, 1 for runtime error, 2 for syntax error

### 1.3 Test262 Runner
- [x] Parse test262 YAML frontmatter (description, flags, features, negative, includes)
- [x] Handle test flags: `onlyStrict`, `noStrict`, `raw`, `module`, `async`, `generated`
- [x] Pre-load harness files (`assert.js`, `sta.js`, `doneprintHandle.js`, etc.)
- [x] Handle negative tests (expected parse/runtime errors)
- [x] Produce JSON/text report: pass/fail/skip per test
- [x] Summary statistics output
- [x] Support running single test, directory, or full suite
- [x] Timeout handling per test (e.g., 10s)
- [x] Parallel test execution

### 1.4 Custom Test Runner
- [x] Script to run tests from `tests/` directory
- [x] Simple pass/fail assertion mechanism

### 1.5 CI / Development Tooling
- [x] GitHub Actions: build + test on PR
- [x] Clippy + rustfmt in CI
- [x] Script to update README.md with latest test262 results

### 1.6 Documentation
- [x] Update README.md with build/run instructions
- [ ] Architecture overview in docs/

## Dependencies
None — this is the foundation.

## test262 Tests
Not directly tested, but the runner itself is validated by running a few known-simple tests once Phase 2+ is ready.
