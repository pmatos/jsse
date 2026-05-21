# JSSE - JavaScript Engine in Rust

## Project Overview
A from-scratch JavaScript engine implemented in Rust. No JS parser/engine libraries allowed as dependencies — every detail must be implemented by us. Utility crates (parsing combinators, math, etc.) are fine.

**GitHub repo: `pmatos/jsse`** — always use this owner/repo for GitHub MCP calls.

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
7. For debugging and comparison, `node` is available as a reference engine. Authority order: (1) ECMAScript spec, (2) test262, (3) node.

## Source Layout
- `src/main.rs` — CLI entry point (`jsse <file>` or `jsse -e "code"`)
- `src/lexer.rs` — Tokenizer
- `src/ast.rs` — AST node types
- `src/types.rs` — JS value types (`JsValue`, `JsString`, `JsSymbol`, `JsBigInt`, etc.)
- `src/parser/` — Recursive descent parser
  - `mod.rs` — Parser struct, parse_program(), utility helpers
  - `expressions.rs` — Expression parsing
  - `statements.rs` — Statement parsing
  - `declarations.rs` — Function, class, variable, destructuring parsing
  - `modules.rs` — Module-specific parsing
- `src/interpreter/` — Tree-walking interpreter
  - `mod.rs` — Interpreter struct, new(), run(), object/property helpers
  - `types.rs` — Completion, Environment, JsFunction, PropertyDescriptor, JsObjectData, etc.
  - `helpers.rs` — Type conversion, equality, JSON, date math helpers
  - `gc.rs` — Mark-and-sweep GC with ephemeron support
  - `exec.rs` — Statement execution (exec_statements, loops, try/switch)
  - `eval.rs` — Expression evaluation (eval_expr, eval_call, eval_new, etc.)
  - `builtins/` — Built-in object setup
    - `mod.rs` — setup_globals, setup_object_statics, setup_reflect, setup_proxy, setup_function_prototype
    - `array.rs` — setup_array_prototype
    - `string.rs` — setup_string_prototype
    - `number.rs` — setup_number_prototype, setup_boolean_prototype, setup_symbol_prototype
    - `iterators.rs` — setup_iterator_prototypes, setup_generator_prototype
    - `collections.rs` — setup_map/set/weakmap/weakset_prototype
    - `date.rs` — setup_date_builtin
- `scripts/` — Test runners and utilities
- `test262-pass.txt` — Regression baseline of currently passing test262 tests. **Not rewritten by default.** The runner reads the baseline from `origin/main:test262-pass.txt` (override with `--baseline-ref`) so feature branches don't conflict on it. Pass `--update-baseline` to rewrite the working-tree file — typically only done on `main` (or a branch targeting it) to roll the baseline forward.
- `test262-extra/` — Custom spec-compliance tests not covered by test262

## Building
- `cargo build --release` — always build in release mode for test262 runs (debug is too slow)
- The project uses Rust nightly features (`let_chains`, etc.)

## Testing
- Primary validation: test262 suite
- Custom tests: `tests/` directory
- After any implementation work, run the full test262 suite.
- Run test262: `uv run python scripts/run-test262.py`
- Run linter: `./scripts/lint.sh`
- Python scripts are run via `uv run python` (no virtualenv setup needed).
- Ensure forward progress.
  - We should implement new features to ensure new tests pass without regressing on previously passing tests.
- Each test runs under a time limit (default 120s) and a memory limit (512 MB) to prevent runaway tests from crashing the system. These limits are enforced in `scripts/run-test262.py`.
- We implement all optional test262 features including intl402 (Intl API) and Temporal. The default test runner covers `language/`, `built-ins/`, `annexB/`, and `intl402/`. Staging tests are tracked separately — run them explicitly with `uv run python scripts/run-test262.py test262/test/staging/`.
- Any validation that's spec-correct but not in test262 should have its own tests in test262-extra/
  - it should include spec part that is tested and follow the exact same patterns of test262 tests.
- Run test262 on a specific directory: `uv run python scripts/run-test262.py test262/test/built-ins/Symbol/`
- Run custom tests: `uv run python scripts/run-custom-tests.py`

## Mutation Testing
- Local-only (not in CI). Driver: `./scripts/run-mutants.sh` (forwards args to `cargo mutants`).
- Requires `cargo install cargo-mutants --locked` and `uv` on PATH (or at `~/.local/bin/uv`).
- Examples:
  - `./scripts/run-mutants.sh --list` — enumerate mutants without running.
  - `./scripts/run-mutants.sh --shard 0/8` — single shard of the corpus.
  - `./scripts/run-mutants.sh --file src/lexer.rs` — restrict to one file.
- Oracle is `cargo test --release` plus `tests/test262_smoke_oracle.rs`, which runs a 0.5% random test262 sample (~3,500 scenarios, ~20 s on a fast machine). The sample is unseeded, so kill verdicts are non-deterministic across mutants — the trade-off is broader cross-section coverage over many runs.
- Configuration in `.cargo/mutants.toml`. Generated tables (`unicode_tables.rs`, `emoji_strings.rs`) and two combinatorially-explosive Temporal helpers (`duration.rs`, `plain_date_time.rs`) are excluded.
- Output in `mutants.out/` (gitignored): `caught.txt`, `missed.txt`, `unviable.txt`, `outcomes.json`, plus per-mutant diffs/logs.

## Acorn Tests
- Run acorn tests: `./scripts/run-acorn-tests.sh`
- Compare with Node baseline: `./scripts/run-acorn-tests.sh --node`
- Force a fresh clone: `./scripts/run-acorn-tests.sh --clean`
- The script clones acorn into `/tmp/acorn`, bundles its test suite with esbuild into a single IIFE, prepends runtime shims (`scripts/acorn-shim.js`), and runs it on jsse.
- Cloned acorn and esbuild bundle are cached in `/tmp/`; use `--clean` to rebuild from scratch.
- **esbuild comment stripping**: esbuild removes comments from function bodies. The `TestComments` test relies on `Function.prototype.toString()` preserving comments, so a pre-bundle patch (`scripts/patch-acorn-comments.js`) replaces the `.toString()` call with a string literal.
- All 13,507 acorn tests should pass on both jsse and Node.

## Architecture Notes
- The interpreter is a single-pass tree-walker over the AST — no bytecode compilation.
- Built-in prototypes (e.g. `string_prototype`, `symbol_prototype`) are stored as fields on `Interpreter` and wired up in `setup_builtins()` / `setup_*_prototype()` methods.
- GC is mark-and-sweep with ephemeron support for WeakMap/WeakSet. Realm prototype fields are still rooted in `gc_safepoint()`. Kind-specific roots are derived from `ObjectKind` via an exhaustive `match` in `gc::trace_object_fields` — adding a new variant fails to compile until the GC walker handles it.
- `ObjectKind` (in `interpreter/types.rs`) is the canonical "what shape is this object?" discriminator. Each variant owns the kind-specific slot data (`ProxyData`, `ArrayBufferData`, `TypedArrayInfo`, `IteratorState`, `PromiseData`, `BoundFunctionData`, `IterHelperData`, `FinalizationRegistry`, `Map`, `Set`, `Arguments`, `Array`, `ModuleNamespace`, `DisposableStack`, `TemporalData`, `IntlData`, `WrappedFunctionData`, `ShadowRealm`, `RegExpData`, `DataView`, `Ordinary`, `PrimitiveWrapper`). The disjunction is type-enforced — an object cannot be e.g. both a Proxy and a TypedArray simultaneously. Cross-cutting aspects (`callable`, `constructor_kind`, `prototype_id`, `properties`) remain orthogonal fields on `JsObjectData`. Predicate methods (`is_proxy`, `is_class_constructor`, `arraybuffer_is_shared`, etc.) and accessor methods (`obj.bound()`, `obj.proxy()`, `obj.typed_array_info()`, etc.) pull from `kind` and are the supported read API; direct `obj.kind` matching is fine when destructuring is wanted.
- Generators use a replay-based approach (re-execute the function body, fast-forwarding past previous yields).
- The `Object()` constructor calls `to_object()` to wrap primitives (String, Number, Boolean, Symbol, BigInt).

## Agent skills

### Issue tracker

Issues live on GitHub at `pmatos/jsse` and are managed via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical label vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout — one `CONTEXT.md` + `docs/adr/` at the repo root (created lazily by `/grill-with-docs`). See `docs/agents/domain.md`.
