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
5. The spec is the ultimate source of truth with respect to JavaScript. Use it to determine the syntax and semantics of operations.
6. For debugging and comparison, `node` is available as a reference engine. Authority order: (1) ECMAScript spec, (2) test262, (3) node.

## Required Hook Setup

Before implementation work and before committing, run `./scripts/ensure-hooks.sh`
and verify success. Launch agent commands through
`./scripts/agent-session.sh --worktree <path> <agent-command> [arguments...]`
so the selected worktree is provisioned before the agent starts there. Omit
`--worktree` to use the current worktree. Setup failures prevent startup.
Provision hooks before making Git metadata read-only and keep `PRE_COMMIT_HOME`
writable. Claude's `SessionStart` hook attempts setup, and its `PreToolUse` gate
blocks edits and shell commands until that worktree's hooks are enabled.

Never bypass required checks with `--no-verify`, `SKIP`, or an overridden
`core.hooksPath`. If hooks cannot be installed or executed, report the cause and
do not commit until the environment is fixed. Preserve existing Git hooks.
The required CI `build` job enforces the token budget on PR changes even if a
local hook was bypassed.

## Source Layout
- `src/main.rs` — CLI entry point (`jsse <file>` or `jsse -e "code"`)
- `src/fuzz_reentrancy/` — `reentrancy` fuzz target: decodes bytes into a plan (op, host, hook, action, step, shape) and renders a JS program whose Proxy traps/generators/accessors/callbacks re-enter the engine; seeds in `fuzz/corpus/reentrancy/` via `scripts/gen-fuzz-reentrancy-corpus.py`
- `src/lexer.rs` — Tokenizer
- `src/ast.rs` — AST node types
- `src/types.rs` — JS value types (`JsValue`, `JsString`, `JsSymbol`, `JsBigInt`, etc.)
- `src/parser/` — Recursive descent parser
  - `mod.rs` — Parser struct, parse_program(), utility helpers
  - `expressions.rs` — Expression parsing
  - `statements.rs` — Statement parsing
  - `declarations.rs` — Function, class, variable, destructuring parsing
  - `modules.rs` — Module-specific parsing
  - `dynamic_function.rs` — CreateDynamicFunction's standalone FormalParameters/FunctionBody parse
- `src/interpreter/` — Tree-walking interpreter
  - `mod.rs` — Interpreter struct, new(), run(), object arena, realm management
  - `types.rs` — Completion, Environment, JsFunction, PropertyDescriptor, JsObjectData, etc.
  - `helpers.rs` — Type conversion, equality, JSON, date math helpers
  - `gc.rs` — Mark-and-sweep GC with ephemeron support
  - `exec.rs` — Statement execution (exec_statements, loops, try/switch)
  - `eval.rs` — Expression evaluation (eval_expr, eval_call, eval_new, etc.)
  - `property.rs` — Property-access MOP operations ([[Get]], [[Set]], [[DefineOwnProperty]], [[Delete]], [[HasProperty]], proxy traps, array/typed-array exotic, chain walkers)
  - `scheduler.rs` — Job scheduler: microtask queue, async-generator/async-function state, and the `setTimeout`/`setInterval` timer queue serviced by the event loop
  - `builtins/` — Built-in object setup
    - `mod.rs` — module declarations and `setup_globals`, which calls each `setup_*` in dependency order
    - `object.rs` — setup_object_constructor, setup_object_statics (Object constructor statics and Object.prototype methods)
    - `reflect.rs` — setup_reflect (the `Reflect` namespace object)
    - `function.rs` — setup_function_prototype (call, apply, bind, toString)
    - `function_constructors.rs` — setup_function_constructors (eval, Function, AsyncFunction, GeneratorFunction, AsyncGeneratorFunction and their prototype wiring)
    - `host.rs` — setup_host_globals (console, print, timers)
    - `host_base64.rs` — `btoa`/`atob` (HTML Latin-1 base64, forgiving decode, InvalidCharacterError)
    - `structured_clone/` — `structuredClone`: `mod.rs` fuses HTML structured serialize/deserialize into one pass (memory map for cycles/shared refs, `transfer` of ArrayBuffers, DataCloneError for uncloneable objects) and roots every clone and source for the whole call because getters run mid-walk; `buffers.rs` clones ArrayBuffer/SharedArrayBuffer and their views
    - `errors.rs` — setup_error_builtins (Error, native errors, AggregateError, SuppressedError)
    - `math.rs` — setup_math (the `Math` namespace object and its f16/precise-sum helpers)
    - `json.rs` — setup_json (the `JSON` namespace object)
    - `shadow_realm.rs` — setup_shadow_realm
    - `primitive_constructors.rs` — Symbol, String, Number and Boolean constructors, Number statics, String.raw, String.fromCharCode
    - `global_functions.rs` — setup_global_functions (parseInt, parseFloat, isNaN, isFinite, URI functions, escape/unescape)
    - `global_object.rs` — setup_global_object (globalThis, global-object wiring, $262)
    - `array.rs` — setup_array_prototype
    - `array_dense.rs` — dense-array fast paths for `Array.prototype` (pop/shift/unshift/splice/slice/concat/reverse/fill/sort write-back and indexOf/includes scans). Each helper acts only on a plain dense Array (`JsObjectData::is_plain_dense_array`: no own property but a synced, writable `length`, so `undefined` backing slots are holes) after re-checking the guard at the point the spec's element loop would start, and returns `None`/`false` without side effects otherwise so the spec-literal loop runs. Holes and growth additionally require `proto_chain_index_free`
    - `merge_sort.rs` — `merge_sort_by`, the stable natural merge sort behind comparefn sorts (`Array`/`%TypedArray%` `sort`/`toSorted`): reverses strictly descending runs, so sorted or reverse-sorted input costs n-1 comparefn calls
    - `string.rs` — setup_string_prototype
    - `number.rs` — setup_number_prototype, setup_boolean_prototype, setup_symbol_prototype
    - `iterators.rs` — setup_iterator_prototypes, setup_generator_prototype
    - `collections.rs` — setup_map/set/weakmap/weakset_prototype
    - `date.rs` — setup_date_builtin
  - `helpers/json_stringify.rs` — `JSON.stringify` serializer (SerializeJSONProperty/QuoteJSONString); roots the holder and each post-`toJSON`/replacer value across nested calls
  - `helpers/json_internalize.rs` — `JSON.parse` reviver walk (InternalizeJSONProperty); roots the holder and walked value across each reviver call
  - `atom.rs` — `Atom`: randomised-interner identifier atoms (pointer equality, precomputed hash)
  - `binding_map.rs` — `BindingMap`: atom-keyed `Environment.bindings` storage and the `BindingKey`/`BindingName` lookup traits
  - `element_vec.rs` — `ElementVec`: Array element storage (`Vec<JsValue>` plus a dead-slot head offset) so `shift` is amortised O(1); derefs to the live `[JsValue]` window
  - `keyed_entries.rs` — `KeyedEntries`: Map/Set/WeakMap/WeakSet [[MapData]]/[[SetData]] storage, an insertion-ordered slot list with a SameValueZero hash index; iterators hold spec-index cursors stepped by `next_live`, so tombstone compaction needs no iterator fix-up
  - `global_access.rs` — Global Data Fast Path: clone-free scope walk plus one re-validated own-property read/write of a global-object data property, with per-site `GlobalHint` slot hints
- `scripts/` — Test runners and utilities
- `test262-pass.txt` — Regression baseline of currently passing test262 tests. **Not rewritten by default.** The runner reads the baseline from `origin/main:test262-pass.txt` (override with `--baseline-ref`) so feature branches don't conflict on it. Pass `--update-baseline` to rewrite the working-tree file — typically only done on `main` (or a branch targeting it) to roll the baseline forward.
- `test262-extra/` — Custom spec-compliance tests not covered by test262

## Building
- `cargo build --release` — always build in release mode for test262 runs (debug is too slow)
- The project uses Rust nightly features (`let_chains`, etc.)
- The release profile is fat LTO + `codegen-units = 1` (#935: +6.4% JetStream 3 score geomean, -5.9% micro-suite wall time). A release build is therefore ~2.5-3x slower than before. For quick iteration that does not need benchmark-grade speed: `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo build --release` (use its own `--target-dir` if you also need the shipped binary). Never compare timings across the two configurations.
- The shipped binary uses jemalloc (`tikv-jemallocator`, default feature `jemalloc`, `#[global_allocator]` in `src/main.rs` only; the lib, unit tests, miri and fuzz targets keep the system allocator; not resolved on Windows/wasm, #1047). Wall time moves in ways callgrind cannot see (kernel time, page faults), so measure allocator changes on the clock and with `/usr/bin/time`. `src/main.rs` embeds `thp:never` (jemalloc's default zeroes a huge page per fresh extent on THP=always hosts: +17% on a test262 run of short processes) and caps glibc at one arena (per-thread 64 MiB reservations starve the Atomics agent tests under the runner's RLIMIT_AS). Build with `--no-default-features` for valgrind dhat/memcheck heap profiling or when comparing against pre-#1047 numbers; callgrind Ir of a default build includes jemalloc frames. On aarch64 hosts with 64 KiB pages set `JEMALLOC_SYS_WITH_LG_PAGE=16` for the build. Measurements: `docs/perf/2026-10-16/1047-jemalloc.md`.
- `release-nolto` (release speed, no LTO) is the profile for building things that are only tested: `cargo nextest run --cargo-profile release-nolto`, `cargo test --profile release-nolto`. `cargo test --release` would relink every test binary with fat LTO. `release-checked` inherits it.

## Conventions
 - PR titles: [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
  PRs are squash-merged by default - clean-history branches.
 - Every change reaches `main` through a PR: the `main` ruleset blocks direct
  pushes for everyone except the release deploy key used by `release.yml`.

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
- Any validation that's spec-correct but not in test262 should have its own tests in test262-extra/. This includes regressions for engine-internal heuristics when the failure changes an observable ECMAScript value or throw; exact host-compatibility diagnostics and engine resource-limit or stress checks remain in `tests/`.
  - it should include spec part that is tested and follow the exact same patterns of test262 tests.
- Run test262 on a specific directory: `uv run python scripts/run-test262.py test262/test/built-ins/Symbol/`
- Run custom tests: `uv run python scripts/run-custom-tests.py`

## GC Stress Mode
- `JSSE_GC_STRESS=N` (any release or debug binary; read once per interpreter) forces a collection at every Nth safepoint that would not otherwise collect, alternating major (finds missing roots) and minor (finds missing write barriers). Unset, `0` or unparsable leaves it off; the off path costs one predictable branch per safepoint.
- Safepoints are statement boundaries and loop back-edges. The bytecode VM reaches both: `Op::Safepoint` fires once before every statement-list entry that compiles to any bytecode (function/script top level and `Statement::Block` bodies; a statement compiling to nothing, e.g. `;` or `var a;`, drops its safepoint too — there's nothing new to collect), matching the tree-walker's per-statement safepoint, in addition to the existing back-edge safepoint on negative-offset `Op::Jump` — so bytecode (the default) stress coverage is close to tree-walker parity for compiled chunks, modulo `CompileGoal`/`CompileError::Ineligible` bail coverage and the AST Fallback nodes (constructs the compiler doesn't accept still run on the tree-walker; a hand-off node's own statements hit the tree-walker's safepoints). Collection never fires at allocation time.
- The test262 runner and integration tests that spawn the binary inherit the variable (in-crate unit tests ignore it and opt in with `GcPacer::set_stress_period`): `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --timeout 300`. `N=1` is only practical on small directories; use `N=16..1000` with `--sample`/`--seed` for broad runs.
- The in-process `gc_stress` fuzz target applies a per-input period via `jsse::run_script_gc_stressed` (ADR `docs/adr/2026-10-23-1800-gc-stress-fuzz-target.md`).
- Triage: a stress-only failure means a live value was unreachable from `collect_gc_roots` at a safepoint (a freed object whose arena id was recycled usually shows up as a wrong-typed value or `TypeError`). Reproduce with a minimal script plus `$262.gc()`, then diff against the same run without the variable. Timeouts under stress are cost, not GC bugs. Never `--update-baseline` under stress.
- `ci.yml` runs a blocking `JSSE_GC_STRESS=7` pass over `test262-extra/` (normal and `--no-bytecode`) on every PR/push, reusing the plain release binary (#809). The unflagged pass exercises the now-default bytecode VM; the `--no-bytecode` pass keeps the tree-walker covered.
- `nightly-gc-stress.yml` runs a non-blocking, higher-intensity (`N=2`) `--sample 0.003` stress pass over the full `test262/` suite daily, seeded from the run id so a red run stays reproducible; the tiny sample is what keeps `N=2` affordable despite the `N=16..1000` broad-run guidance above (#809).

## GC Root-Stack Discipline
- `gc_temp_roots` is a strictly LIFO `RootStack`; roots are released in reverse order of push (`gc_unroot_id` debug-asserts the id is on top, frames truncate). Persistent captures belong in a Pinned Native Root or `RootedSlots`, never on the stack. Balance is also asserted after every native call, microtask/timer job and program run (`gc_assert_root_depth`).
- `gc_bytecode_roots` (the bytecode VM's operand-stack roots, pushed/popped from `bytecode/vm.rs`) shares the same `RootStack` type, so it gets the same LIFO-release assert for free. It additionally gets its own chunk-exit balance check in `run_chunk_with_var_prologue`: `debug_assert_eq!` against the entry depth, skipped for `Throw`/`Exit` completions because a mid-chunk abrupt exit can legitimately leave operands from an outer in-progress expression still rooted (issue #331) — the unconditional `truncate` right after handles releasing those. It does not yet get `gc_assert_root_depth`-style equality checks at the native-call/microtask-job/timer-callback/program-run boundaries that `gc_temp_roots` has (#810) — every nested `run_chunk` call already truncates unconditionally before returning past those boundaries, so adding the checks is sound, just not done yet (issue #807 follow-up).
- These are `debug_assert!`s. Run them at release speed with `cargo build --profile release-checked` and `uv run python scripts/run-test262.py --binary target/release-checked/jsse ...`; CI does this for test262-extra (normal and `--no-bytecode`) and the 10% sample. The checked binary has debug-sized recursion/parse-depth limits (`cfg!(debug_assertions)`), so keep `tests/recursion-limit-*.js` on the plain release build. `cargo test` (debug) executes them too.

## Long-Running Builds & Tests
- Never rebuild the binary while a test262 / full-suite run is in flight; snapshot the binary to a temp path first or wait for completion.
- Never use `pkill -9 <pattern>` — patterns have matched Claude's own shell. Kill by recorded PID only, and clean up temp files explicitly.
- Clear stale compile caches before declaring a repro 'still failing'.
- Long compiles exceed the default 2-minute Bash timeout: pass an explicit longer timeout up front.

## Execution Counters (performance investigations)

Before a performance investigation, read [Execution Counters](docs/agents/execution-counters.md) for the required build and measurement discipline.

## Mutation Testing
- Local-only (not in CI). Driver: `./scripts/run-mutants.sh` (forwards args to `cargo mutants`).
- Requires `cargo install cargo-mutants --locked` and `uv` on PATH (or at `~/.local/bin/uv`).
- Examples:
  - `./scripts/run-mutants.sh --list` — enumerate mutants without running.
  - `./scripts/run-mutants.sh --shard 0/8` — single shard of the corpus.
  - `./scripts/run-mutants.sh --file src/lexer.rs` — restrict to one file.
- Oracle is `cargo test --profile release-nolto` plus `tests/test262_smoke_oracle.rs`, which runs a 0.5% random test262 sample (~3,500 scenarios, ~20 s on a fast machine). The sample is unseeded, so kill verdicts are non-deterministic across mutants — the trade-off is broader cross-section coverage over many runs.
- Configuration in `.cargo/mutants.toml`. Generated tables (`unicode_tables.rs`, `emoji_strings.rs`) and two combinatorially-explosive Temporal helpers (`duration.rs`, `plain_date_time.rs`) are excluded.
- Output in `mutants.out/` (gitignored): `caught.txt`, `missed.txt`, `unviable.txt`, `outcomes.json`, plus per-mutant diffs/logs.

## Library Tests (Node-compat harness)

Before running or extending library tests, read [Library Tests](docs/agents/library-tests.md) for harness instructions and known results.

## Architecture Notes

Before changing the interpreter, read the relevant architecture instructions:

- [VM, functions, and environments](docs/agents/architecture-vm.md).
- [Objects, GC, scheduling, and built-ins](docs/agents/architecture-runtime.md).

## Agent skills

### Issue tracker

Issues live on GitHub at `pmatos/jsse` and are managed via the `gh` CLI; external PRs are not a triage surface (issues only). See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical triage roles map to the tracker labels `needs-triage`, `needs-info`, `agent-ready`, `human-ready`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout — one `CONTEXT.md` + `docs/adr/` at the repo root (created lazily by `/domain-modeling`). See `docs/agents/domain.md`.
