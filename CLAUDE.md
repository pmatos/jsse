# JSSE - JavaScript Engine in Rust

## Project Overview
A from-scratch JavaScript engine implemented in Rust. No JS parser/engine libraries allowed as dependencies — every detail must be implemented by us. Utility crates (parsing combinators, math, etc.) are fine.

**Ultimate goal: 100% test262 pass rate.**

## Repository Layout
- `spec/` — ECMAScript spec submodule (tc39/ecma262). **NEVER modify.**
- `test262/` — Test suite submodule (tc39/test262). **NEVER modify.**
- `tests/` — Custom tests that don't fit test262.

## Key Rules
1. **Never modify** files in `spec/` or `test262/` submodules.
2. No importing a JS parser or engine crate. Implement everything from scratch.
3. Dependencies for parsing utilities, math, etc. are allowed.
4. When implementing a feature, identify relevant test262 tests to validate against.
5. After running test262, update `README.md` with pass count and percentage.
6. The spec is the ultimate source of truth with respect to JavaScript. Use it to determine the syntax and semantics of operations.

## Source Layout
- `src/main.rs` — CLI entry point (`jsse <file>` or `jsse -e "code"`)
- `src/lexer.rs` — Tokenizer
- `src/ast.rs` — AST node types
- `src/parser.rs` — Recursive descent parser
- `src/types.rs` — JS value types (`JsValue`, `JsString`, `JsSymbol`, `JsBigInt`, etc.)
- `src/interpreter.rs` — Tree-walking interpreter, all built-ins, GC, runtime (~16k lines)
- `scripts/` — Test runners and utilities
- `plan/` — Per-phase implementation plans
- `test262-pass.txt` — Tracks currently passing test262 tests (updated by the test runner)
- `test262-extra/` — Custom spec-compliance tests not covered by test262

## Building
- `cargo build --release` — always build in release mode for test262 runs (debug is too slow)
- The project uses Rust nightly features (`let_chains`, etc.)

## Testing
- Primary validation: test262 suite
- Custom tests: `tests/` directory
- After any implementation work, run the full test262 suite and update README.md progress.
- Run test262: `uv run python scripts/run-test262.py`
- Run linter: `./scripts/lint.sh`
- Python scripts are run via `uv run python` (no virtualenv setup needed).
- Ensure forward progress.
  - We should implement new features to ensure new tests pass without regressing on previously passing tests.
- Each test runs under a time limit (default 60s) and a memory limit (512 MB) to prevent runaway tests from crashing the system. These limits are enforced in `scripts/run-test262.py`.
- Any validation that's spec-correct but not in test262 should have its own tests in test262-extra/
  - it should include spec part that is tested and follow the exact same patterns of test262 tests.
- Run test262 on a specific directory: `uv run python scripts/run-test262.py test262/test/built-ins/Symbol/`
- Run custom tests: `uv run python scripts/run-custom-tests.py`
- After implementation, also update `PLAN.md` with new pass counts for affected built-ins.

## Architecture Notes
- The interpreter is a single-pass tree-walker over the AST — no bytecode compilation.
- Built-in prototypes (e.g. `string_prototype`, `symbol_prototype`) are stored as fields on `Interpreter` and wired up in `setup_builtins()` / `setup_*_prototype()` methods.
- GC is mark-and-sweep with ephemeron support for WeakMap/WeakSet. Prototype fields must be added to the root set in `maybe_gc()`.
- Generators use a replay-based approach (re-execute the function body, fast-forwarding past previous yields).
- The `Object()` constructor calls `to_object()` to wrap primitives (String, Number, Boolean, Symbol, BigInt).
