# Phase 1: Project Scaffolding & Infrastructure

## Goal
Set up the Rust project, CLI interface, test262 runner, and CI pipeline.

## Tasks

### 1.1 Rust Project Setup
- [ ] `cargo init` with binary crate
- [ ] Set up workspace if needed (e.g., `jsse-core`, `jsse-cli`)
- [ ] Choose Rust edition (2024)
- [ ] Set up `Cargo.toml` with initial dependencies
- [ ] Add `.gitignore` for Rust

### 1.2 CLI Interface
- [ ] Accept JS file path as argument
- [ ] Accept `--eval` / `-e` for inline JS
- [ ] REPL mode when no arguments given
- [ ] `--version` and `--help` flags
- [ ] Exit codes: 0 for success, 1 for runtime error, 2 for syntax error

### 1.3 Test262 Runner
- [ ] Parse test262 YAML frontmatter (description, flags, features, negative, includes)
- [ ] Handle test flags: `onlyStrict`, `noStrict`, `raw`, `module`, `async`, `generated`
- [ ] Pre-load harness files (`assert.js`, `sta.js`, `doneprintHandle.js`, etc.)
- [ ] Handle negative tests (expected parse/runtime errors)
- [ ] Produce JSON/text report: pass/fail/skip per test
- [ ] Summary statistics output
- [ ] Support running single test, directory, or full suite
- [ ] Timeout handling per test (e.g., 10s)
- [ ] Parallel test execution

### 1.4 Custom Test Runner
- [ ] Script to run tests from `tests/` directory
- [ ] Simple pass/fail assertion mechanism

### 1.5 CI / Development Tooling
- [ ] GitHub Actions: build + test on PR
- [ ] Clippy + rustfmt in CI
- [ ] Script to update README.md with latest test262 results

### 1.6 Documentation
- [ ] Update README.md with build/run instructions
- [ ] Architecture overview in docs/

## Dependencies
None — this is the foundation.

## test262 Tests
Not directly tested, but the runner itself is validated by running a few known-simple tests once Phase 2+ is ready.
