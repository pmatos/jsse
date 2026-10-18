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
    - `errors.rs` — setup_error_builtins (Error, native errors, AggregateError, SuppressedError)
    - `math.rs` — setup_math (the `Math` namespace object and its f16/precise-sum helpers)
    - `json.rs` — setup_json (the `JSON` namespace object)
    - `shadow_realm.rs` — setup_shadow_realm
    - `primitive_constructors.rs` — Symbol, String, Number and Boolean constructors, Number statics, String.raw, String.fromCharCode
    - `global_functions.rs` — setup_global_functions (parseInt, parseFloat, isNaN, isFinite, URI functions, escape/unescape)
    - `global_object.rs` — setup_global_object (globalThis, global-object wiring, $262)
    - `array.rs` — setup_array_prototype
    - `array_dense.rs` — dense-array fast paths for `Array.prototype` (pop/shift/unshift/splice/slice/concat/reverse/fill/sort write-back and indexOf/includes scans). Each helper acts only on a plain dense Array (`JsObjectData::is_plain_dense_array`: no own property but a synced, writable `length`, so `undefined` backing slots are holes) after re-checking the guard at the point the spec's element loop would start, and returns `None`/`false` without side effects otherwise so the spec-literal loop runs. Holes and growth additionally require `proto_chain_index_free`
    - `string.rs` — setup_string_prototype
    - `number.rs` — setup_number_prototype, setup_boolean_prototype, setup_symbol_prototype
    - `iterators.rs` — setup_iterator_prototypes, setup_generator_prototype
    - `collections.rs` — setup_map/set/weakmap/weakset_prototype
    - `date.rs` — setup_date_builtin
  - `atom.rs` — `Atom`: randomised-interner identifier atoms (pointer equality, precomputed hash)
  - `binding_map.rs` — `BindingMap`: atom-keyed `Environment.bindings` storage and the `BindingKey`/`BindingName` lookup traits
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
- `JSSE_GC_STRESS=N` (any release or debug binary; read once per interpreter) forces a collection at every Nth safepoint that would not otherwise collect, alternating major (finds missing roots) and minor (finds missing write barriers). Unset, `0` or unparseable leaves it off; the off path costs one predictable branch per safepoint.
- Safepoints are statement boundaries and loop back-edges. The bytecode VM reaches both: `Op::Safepoint` fires once before every statement-list entry that compiles to any bytecode (function/script top level and `Statement::Block` bodies; a statement compiling to nothing, e.g. `;` or `var a;`, drops its safepoint too — there's nothing new to collect), matching the tree-walker's per-statement safepoint, in addition to the existing back-edge safepoint on negative-offset `Op::Jump` — so bytecode (the default) stress coverage is close to tree-walker parity for compiled chunks, modulo `CompileGoal`/`CompileError::Ineligible` bail coverage and the AST Fallback nodes (constructs the compiler doesn't accept still run on the tree-walker; a hand-off node's own statements hit the tree-walker's safepoints). Collection never fires at allocation time.
- The test262 runner and integration tests that spawn the binary inherit the variable (in-crate unit tests ignore it and opt in with `GcPacer::set_stress_period`): `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --timeout 300`. `N=1` is only practical on small directories; use `N=16..1000` with `--sample`/`--seed` for broad runs.
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
- Off by default and absent from the shipped binary. Build with
  `cargo build --profile release-nolto --features perf-counters` (counts are
  deterministic, so skip the fat-LTO relink; the binary is
  `target/release-nolto/jsse`); every run **that executes
  JavaScript** then writes tab-separated counters to **stderr** at exit — file,
  `-e`, `--prelude`, and REPL alike, including a prelude that throws (covered by
  `tests/perf_counters_report_paths.rs`). A run that dies before any interpreter
  exists — an unreadable main file on the no-prelude path — reports nothing,
  deliberately: there are no counters to report, and an all-zero block would
  read as a completed measurement of nothing.
- Reports the compiled/tree-walker split by **work** (VM opcodes vs
  `exec_statement`/`eval_expr` entries), not just by invocation — the two
  disagree by an order of magnitude on real code, see
  `docs/perf/2026-08-26/mandreel-bytecode-work-share.md`.
- `global_prop_fast_hits` / `global_prop_fast_misses` count bare-name reads and
  writes served by the Global Data Fast Path (`global_access.rs`) and those that
  fell back to the generic Object Environment Record path (an accessor,
  non-writable or inherited property, a `with` scope, a Proxy global).
- `local_slot_hits` / `local_slot_misses` count `LoadLocal`/`StoreLocal`/
  `StoreLocalPop`/`UpdateLocal`/`LocalBinConst` executions whose slot hint
  matched or fell back to by-name resolution; `atom_intern_calls` / `atom_lookup_calls` count entries into the
  hashing identifier interner. A call path that is free of identifier hashing
  keeps the two atom counters flat as the call count grows
  (`tests/perf_counters_env_hashing.rs`).
- `PERF` totals, `OP` per-opcode histogram, `BAIL` compile-bail reasons named by
  AST variant (`statement:Labeled`) for bodies that stay wholly on the
  tree-walker, `FALLBACK` static hand-off nodes per reason inside *compiled*
  bodies (one row count per literal), `PERF ast_fallback_execs` /
  `ast_units_in_fallbacks` (dynamic hand-offs and the tree-walker work they
  spend, attributed to the enclosing function's `BODY` row but not counted as
  an AST-body invocation), `BODY` per-function tree-walker work units spent
  *exclusive* of nested bodies, plus GC collection counts and time. A function
  with no `BAIL` can still run hot constructs on the tree-walker: read
  `FALLBACK`/`ast_units_in_fallbacks` before calling a body "compiled".
- Generator/async state-machine bodies do not pass through `dispatch_body`, but
  retain their originating function identity for attribution, so their work is
  reported under individual function names and kept off the calling function's
  exclusive total. Top-level script, module, and `eval` bodies have no function
  identity and remain in the synthetic `BODY` rows `<script body>`,
  `<module body>`, and `<eval>`.
- `BODY` rows are keyed by function *identity*, not name. Two functions sharing
  a name render as `name#<object id>`; a unique name stays bare, so only genuine
  ambiguity costs readability.
- `compile_ok` and the `BAIL` table cover **script** compile attempts as well as
  function bodies, so a script-only workload no longer reports zero for both.
  A compiled script counts as a `body_non_function_execs` execution, never as a
  `body_dispatch_compiled` invocation. A function body's verdict is shared by
  every closure of its literal (#900), so `compile_ok` and `BAIL` count each
  literal once (again only after a memo eviction), while every closure's `BODY`
  row still names the bail reason.
- `body_dispatch_compiled` / `body_dispatch_ast` count **function invocations
  only** — that is the split #524 published, so keep it comparable. Executions
  of the non-`dispatch_body` paths are reported separately as
  `body_non_function_execs`, which counts **executions, not invocations**:
  generators replay, so one generator call with N yields registers roughly 4N
  state-machine steps there. The execution count on each resolved
  generator/async `BODY` row has the same state-machine-step semantics, not a
  function-call count; replayed work is real work and is counted each time.
- `ast_units_per_ast_body` divides **function** work by function invocations —
  its numerator is `ast_units_in_functions`, not the run-wide `ast_work_units`,
  which also covers script/generator/module/eval work.
- Every count is deterministic, so a shared/loaded host does not compromise it.
  **Never time an instrumented build** — take counts from the feature build and
  wall times from the default build.
- JetStream per-phase driver: `scripts/gen-mandreel-phases.py <mandreel.js> -o
  driver.js [--subphases]`. Op-mix microbenchmark:
  `benchmarks/scripts/bench_opmix.js`.

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
Real-world npm libraries are run as engine stress tests via a generalized
harness. See `scripts/README.md` for the full recipe and how to add a library.
- Run one library: `./scripts/run-library-tests.sh <lib>` (e.g. `decimal.js`, `big.js`, `acorn`).
- Run on Node only (reference oracle): `./scripts/run-library-tests.sh <lib> --node`.
- Force a fresh clone/rebuild: `./scripts/run-library-tests.sh <lib> --clean`.
- Each library has a config at `scripts/libs/<lib>.sh` (pinned repo/ref, bundle entry, prepare hook, verdict). The runner clones the pinned ref, bundles with a pinned esbuild, prepends the shared JS-only Node-globals shims (`scripts/node-shim.js` for `process`/`console`, `scripts/node-buffer-shim.js` for `Buffer`/`TextEncoder`/`TextDecoder`) — never baked into jsse's globals, so test262 is unaffected — runs on jsse, and cross-checks the test count against Node. Caches live under `/tmp/jsse-libtests/<lib>/`.
- The shim stubs are guarded so they are inert on Node, letting `--node` run the identical bundle as the reference.
- **`Buffer`** is a pure-JS subclass of `Uint8Array` (`node-buffer-shim.js`), needing zero new engine object kinds; `instanceof Uint8Array` holds. Self-verifying fixtures live in `scripts/shim-fixtures/`; run them (jsse + Node cross-checked) with `./scripts/run-shim-fixtures.sh`.
- **Currently green (cross-checked vs Node):** `decimal.js` (22,624), `big.js` (47,456; ~7 min — heavy arbitrary-precision arithmetic on the tree-walker), `acorn` (13,507), `prismjs` (2,563), `uglify-js` (4,233; ~15 min — full compress DSL transform/mangle/codegen corpus), `highlight.js` (731; ~30 min for the full auto-detection competition set), `uuid` (75; pinned to the browser build so v3/v5 exercise its own pure-JS MD5/SHA-1 and v1/v4/v6/v7 draw randomness from a `crypto.getRandomValues` shim backed by `__host_random_bytes` — see `node-crypto-shim.js`).
- **`bignumber.js`** is wired but blocked on a jsse bug it surfaced (strict-mode `return <call-returning-non-object>` in a constructor returns that value instead of `this`, jsse#238); it goes green once that engine bug is fixed.
- **`luxon`** is wired at 3.7.2 with an exact 1,152-test Node cross-check. Node itself fails 13/1,152 on this host (ICU/Node version drift, not a jsse bug); jsse currently passes 1,002, with the remaining Intl/system-zone gaps tracked in jsse#262–#265.
- **`zod`** is wired at v4.4.3 in separate normal and jitless processes with an exact 2,184-case Node cross-check. Node is green; jsse runs every case with no skips and reports 2,178 passing, with the 6 residual failures tracked in jsse#313 (1 per mode) and jsse#314 (2 per mode). jsse#310's artificial `500 → 0` refinement-delay patch in `gen-zod-entry.js` is still in place.
- **`moment`** is wired at 2.30.1 with an exact 3,871-test Node cross-check. Node is green with 162,868 assertions; jsse executes every test and records 198 visible failing assertions, tracked in jsse#311.

### Acorn (`./scripts/run-acorn-tests.sh` — thin wrapper over the harness)
- Pinned to **acorn 8.16.0** (13,507 tests, green on jsse and Node). 8.17.0+ added a parser stack-guard test (`"[".repeat(2000)`) that expects the engine to *throw* a "stack space" error; jsse's tree-walker aborts (SIGABRT) before acorn's guard fires — a jsse deep-recursion robustness gap tracked separately. Bump the pin once that lands.
- **esbuild comment stripping**: esbuild removes comments from function bodies. The `TestComments` test relies on `Function.prototype.toString()` preserving comments, so a pre-bundle patch (`scripts/patch-acorn-comments.js`) replaces the `.toString()` call with a string literal.

## Architecture Notes
- The interpreter is a single-pass tree-walker over the AST. A bytecode compiler/VM in `bytecode/` is the default execution mode (`bytecode_enabled` defaults on; `--no-bytecode` opts out); in Function goal a construct the compiler cannot lower falls back to the tree-walker per *node* (`Op::EvalAst`/`Op::ExecAst`, ADR-2026-10-12-2214), and only `CompileError::Ineligible` constructs (direct `eval`, `with`, `super`, `new.target`, compiler resource limits, a body that is nothing but hand-offs) keep a whole function body on the tree-walker; a `yield`/`await` the scan meets can only sit inside a nested arrow, which owns it, so it is not one of them (#907). The Script goal falls back per node too, with valued completions (`EXEC_SET_COMPLETION`, ADR-2026-10-13-1210): it stays whole-body only for direct `eval`, `with`, `import()`/`import.meta` and a script that is nothing but hand-offs. Native lowerings include `switch`, `throw`, `typeof`, member `++`/`--`, `instanceof`, `in`, private-name accesses (`GetPrivate`/`SetPrivate`/`UpdatePrivate`/`LoadMethodPrivate`/`PrivateIn`; each frame resolves a chunk private-name index to its per-class-evaluation `PrivateId` once), BigInt literals and function-top-level `let`/`const`; block, `for (let ...)` and `switch` scopes run in VM-managed block environments (`PushEnv`/`PopEnv`/`CopyIterEnv`, ADR-2026-10-13-0632); array literals (`NewArray`/`NewArrayConst`/`ArrayAppend`), statically keyed object literals (`NewObject`/`ObjectAppend`), regular expression literals (`NewRegExp`) and function/arrow closures with NamedEvaluation (`MakeClosure`) are native too (ADR-2026-10-13-0755); array literals with spread or non-constant elisions, object literals with computed keys or spread, class expressions and destructuring declarations run as tree-walker nodes (see the `FALLBACK` counters). `try`/`catch`/`finally`, `for-in` and `for-of` are native too (ADR-2026-10-13-1019): a per-chunk handler table resumes a `Throw`, a `finally` block is entered once with a completion record `[value, kind]` that `EndFinally` re-dispatches, and a for-of's iterator is an operand-stack slot closed by `IterClose`/`IterCloseThrow`; a tree-walker hand-off inside one of those regions that would complete with a `return`/`break`/`continue` out of it keeps the whole construct on the tree-walker (`completion leaves a cleanup region`). At Script top level they remain tree-walker units. Property-access MOP operations ([[Get]], [[Set]], [[DefineOwnProperty]], [[Delete]], [[HasProperty]], proxy traps, array/typed-array exotic) are consolidated in `property.rs`.
- Built-in prototypes (e.g. `string_prototype`, `symbol_prototype`) are stored as fields on `Interpreter` and wired up in `setup_builtins()` / `setup_*_prototype()` methods.
- GC is mark-and-sweep with ephemeron support for WeakMap/WeakSet. Realm prototype fields are still rooted in `gc_safepoint()`. Kind-specific roots are derived from `ObjectKind` via an exhaustive `match` in `gc::trace_object_fields` — adding a new variant fails to compile until the GC walker handles it.
- Generational write barrier granularity (#912): a remembered old object is re-scanned whole by the next minor collection, except an Array, whose element vector carries a dirty-card bitmap (`ArrayData.dirty_cards`, `CARD_ELEMENTS` = 512 elements, 4 in debug builds so unit tests and stress runs cross card boundaries). `None` means every card is dirty and is the state of every new Array and of any Array after `JsObjectData::array_elements_mut()` (the only general mutable accessor, so an operation that moves or rewrites elements without naming cards can never leave a stale bitmap). Only a single-slot store that ran `Interpreter::gc_write_barrier_element` first (`numeric_index_fast_set`, the tree-walker's dense overwrite/append, `Array.prototype.push`) may use the non-flagging `overwrite_array_element`/`apply_dense_array_set`/`extend_array_elements_precise`. `ObjectHandle::remember_if_old` (conservative, whole object) sets `REMEMBERED_FIELDS`; `remember_elements_if_old` queues an Array for its dirty cards only. A minor scan of a card-tracked Array visits only its dirty cards and, having seen every young reference, keeps exactly the cards that still hold one; a full scan converts `None` into a bitmap for Arrays longer than one card. Minor collections build a `YoungIds` bitmap of the nursery so "is this child young" costs one bit, not a header load, and promotion remembers an object only if tracing saw a young child (or its kind has edges the tracer cannot see). Debug builds assert after marking that no old Array element (or strong Map/Set entry) references an unmarked nursery object. Strong Maps and Sets follow the same rule over `KeyedEntries` slots (#988; `KeyedEntries.dirty_cards`, one card per `CARD_ELEMENTS` physical slots): every method that adds an edge without naming a slot (`upsert`) or moves slots (`compact`, `clear`) resets the bitmap to all-dirty, and a `Map.prototype.set`/`getOrInsert`/`getOrInsertComputed`/`Set.prototype.add` store calls `Interpreter::gc_write_barrier_entry` then `KeyedEntries::upsert_marking`. `JsObjectData::has_card_tracking` is the predicate (Array, strong Map, strong Set); WeakMap/WeakSet are never carded (they are re-scanned whole through the ephemeron path). `trace_object_fields_in(.., bulk = false)` leaves both an Array's elements and a strong Map/Set's entries to the card scan.
- `ObjectKind` (in `interpreter/types.rs`) is the canonical "what shape is this object?" discriminator. Each variant owns the kind-specific slot data (`ProxyData`, `ArrayBufferData`, `TypedArrayInfo`, `IteratorState`, `PromiseData`, `BoundFunctionData`, `IterHelperData`, `FinalizationRegistry`, `Map`, `Set`, `Arguments`, `Array`, `ModuleNamespace`, `DisposableStack`, `TemporalData`, `IntlData`, `WrappedFunctionData`, `ShadowRealm`, `RegExpData`, `DataView`, `Ordinary`, `PrimitiveWrapper`). The disjunction is type-enforced — an object cannot be e.g. both a Proxy and a TypedArray simultaneously. Cross-cutting aspects (`callable`, `constructor_kind`, `prototype_id`, `properties`) remain orthogonal fields on `JsObjectData`. Predicate methods (`is_proxy`, `is_class_constructor`, `arraybuffer_is_shared`, etc.) and accessor methods (`obj.bound()`, `obj.proxy()`, `obj.typed_array_info()`, etc.) pull from `kind` and are the supported read API; direct `obj.kind` matching is fine when destructuring is wanted.
- Per-literal call-frame facts live in `LiteralFrame` (`literal_frame.rs`, #1062): created with a literal's `FunctionLiteralInfo`, shared by `Rc` with every closure (`JsFunction::User.frame`), and holding the interned simple-parameter atoms (plus whether they are fresh, i.e. distinct and not `arguments`, so binding can `push_unique`) and the `BodyStoreHandle` of the literal's IC store. `IcStore::enter_body_cached` revalidates the handle by slot generation and Body identity; functions built without a literal (`Function` constructor and siblings) own a fresh frame.
- `Environment.bindings` is a `BindingMap` (`binding_map.rs`) of interned `Atom`s (`atom.rs`): linear scan up to 16 entries, identity-hashed atom index above that, `this` held in `Environment.this_binding`, and the `arguments` binding declared only when the body can observe it. The interner is the only place identifier text is hashed (randomised, see `docs/specs/2026-07-25-environment-binding-hashing-design.md`). Compiled function bodies use `LoadLocal`/`StoreLocal`/`UpdateLocal` with slot hints verified by atom identity (ADR-2026-10-12-0900). The frame layout is `arguments`, the simple parameters, the var-prologue names, the top-level function-declaration names and the top-level `let`/`const`/class names, in the order declaration instantiation creates them (`compiler::instantiated_names`, only when every `var` name is in the prologue and the parameters are simple); `InitLexical` takes the same slot at function top level (#1022). A free name that is not a frame local keeps a `ScopeHint` in its `GlobalHint`: the hops from the nearest function scope plus the binding slot (or a global-object marker with the key-set stamp of the global environment's indexed `BindingMap`), re-verified on every use by `global_access::walk_scope_chain_hinted` (each skipped scope must not bind the name, one name-filter probe each) and retrained by a full walk when it fails. A free name that resolves to an own data property of the global object takes the Global Data Fast Path (ADR-2026-10-12-1500): `PropertyMap`'s spilled storage is a dense entry list plus a key→slot index so each bytecode name site can remember a verified slot (`Chunk.global_hints`).
- Sync generators whose body the compiler can host run as suspending bytecode chunks (`IteratorState::BytecodeGenerator`, ADR-2026-10-17-0640): `compile_generator_body` is the Function goal with `Op::Yield`, which returns `Completion::Yield` from `run_chunk_inner` after saving the position and block environments into the generator's `GeneratorFrame` (the operand and reference stacks are the frame's own vectors); `run_generator_chunk` resumes it, and `bytecode_generator_resume` is the single `next`/`return`/`throw` implementation. `return(v)` resumes at the stub after the yield, the exit a `return` statement would emit there; `throw(e)` resumes through the ordinary handler table. A body containing `yield*`, `using`, or a `yield` inside a node the compiler hands to the tree-walker (`EvalAst`/`ExecAst`, a head-binding pattern) is `Ineligible` and keeps the state machine, as does every generator under `--no-bytecode` and every async generator and async function. Keep `JsObjectData`-visible changes to the suspended frame traced in `collect_iterator_state_roots`.
- Generators that stay on the state machine are compiled by `generator_transform.rs` into a `GeneratorStateMachine` (`generator_runtime.rs`): each resume jumps straight to the stored state id and suspends via `StateTerminator::Yield`/`Await`, with no replay. A `self.generator_context`/`SentValueBindingKind::InlineYield` fallback still exists in `generator_runtime.rs` for any construct the transform's analysis doesn't (yet) decompose — it re-executes the current state's statement list and fast-forwards past already-fired yields — but it is a degraded-behavior backstop for undiscovered gaps, not the primary mechanism (see issue #625). In async generators an inline `yield`/`yield*` suspends through the same terminator machinery as a lowered one (no blocking `await_value`, see ADR-2026-09-21-2157); only the replay of the state's preceding statements remains.
- The generator/async lowering emits three AST forms the parser never produces, so that evaluation order matches the spec across a suspension without observable lookups: `Expression::BoundCallee(func, this)` (a call or tagged-template callee whose function was already read into a temp; `eval_call`, the tagged-template arm and the bytecode `compile_call` resolve it without a property read), `Expression::PreIterated(array)` under a `Spread` (a fresh array built by one earlier iteration, spread by copying its elements through `spread_values` instead of iterating it again), and `FunctionDecl.phase` (`DeclPhase::InstantiateOnly` at the head of a block's first state creates the function and skips the Annex B.3.3 var copy; `CopyOnly` stays where the declaration is reached, is skipped by `hoist_function_decl`, and performs the copy). `compile_expr` treats the first two as non-standalone, so a parent that cannot lower them natively falls back as a whole.
- Iterator steps in for-of, array destructuring, spread, `Array.from`, the `Map`/`Set` constructors and `Object.fromEntries` go through `Interpreter::iterator_step_value` (`builtins/iterators.rs`). When the `next` that GetIterator captured (`iterator_next_cache`) is one of the current realm's `Realm.iterator_next_intrinsics` (Array, String, Map, Set, Generator `next`; GC roots, so an id can never be recycled into a false match), the step runs that iterator's raw step function (`array_iterator_step`, `string_iterator_step`, `map_iterator_step`, `set_iterator_step`, or `generator_step_value`, which asks the resumption to park `(value, done)` in `raw_iter_step` instead of building the object) and no `{ value, done }` object exists; the native `next` is a thin wrapper over the same raw step, so each iterator kind has one implementation. Any other `next`, a cross-realm iterator, or a `yield*`-forwarded inner result takes the spec-literal call + `done`/`value` Get path. Observable behaviour is unchanged: patching `next` or `@@iterator` mid-loop only affects later loops because the captured `[[NextMethod]]` is what is compared. Async iteration (`for await`) still goes through result objects.
- Closures of one function literal share an `Rc<FunctionLiteralInfo>` (parameter patterns, `uses_arguments`, `has_simple_params`) held in the AST node's `LiteralInfoCell`, alongside the per-`Body` `BodyMemo`s (#900). The cell is filled on first closure creation and `Clone` yields an empty cell, so a cloned-then-mutated node never carries a stale value; a path that clones the node per evaluation (e.g. the per-step `StateTerminator` clone in generator/async state machines) only loses sharing, never correctness.
- Dense Array element access (`numeric_index_fast_get`/`numeric_index_fast_set` in `property.rs`, called from the bytecode `GetElement`/`SetElement` (the VM's `SetElement` uses the `#[inline(always)]` `numeric_index_fast_set_inlined`), the tree-walker's `eval_assign` and `Interpreter::update_member_computed`, which the VM's `UpdateElement` shares; a Number key never builds its key string unless a helper declines): `PropertyMap::may_hold_index_key` is false while no digit-leading key is present (a count maintained by `insert`/`push_fresh`/`remove`), which proves no descriptor shadows the element vector and skips formatting the index. A store is a direct `Vec` write for a present element (`undefined` values and anything with a descriptor take the generic [[Set]]), and a hole fill or append additionally requires an extensible, `length`-synced array whose prototype chain is ordinary/Array objects without index keys (`array_dense::proto_chain_index_free`). Array literals store only present `undefined` elements as descriptors, like `create_array`, so an in-vector `undefined` without a descriptor is a hole.
- Named property access in the bytecode VM (`GetProp`/`LoadMethod`/`SetProp`/`UpdateProp`, `get_prop_op` & co. in `vm.rs`, #1029): `Interpreter::named_get_fast`/`named_store_fast`/`named_update_fast` (`property.rs`) complete a data-property access in place, bypassing the generic [[Get]]/[[Set]] chain, when the receiver and every prototype up to the holder are ordinary. `ObjectKind::is_named_access_exotic` (an exhaustive `match`: classify every new kind there) plus `named_miss_is_ordinary` (String wrappers) decide what is ordinary; Array `length`, Array-index-like names, accessors on a store, non-writable properties and non-extensible receivers fall back (`own_named_record_mut`). An add runs only when the prototype chain does not mention the name and must bump `shape_id` (`add_named_fast`); a store that lands runs `gc_write_barrier_value`. A getter found on the chain is returned (`NamedGet::Getter`) and called with the original receiver, so the generic path never re-walks. The four ops stay out of line: inlining them into `run_chunk_inner` changed the dispatch loop's code generation and cost 2-4% on workloads that never touch a named property.
- Function objects are built by `create_function_with` from pre-interned `FunctionKeys` via `JsObjectData::push_fresh_property` (no per-closure key hashing; `tests/perf_counters_function_creation.rs` gates `key_intern_calls`). `PropertyMap` stays a linear scan up to `LINEAR_MAX = 8` entries before hashing. `JsObjectData.function_realm()` replaces the old function-to-realm map; `None` keeps the caller's realm. `class_name` is a `&'static str`: assign a literal for static tags (the native error constructors map their name to one).
- `JsObjectData` is capped at 320 bytes (`js_object_data_size_cap` in `property_map.rs`, #909): every object is one arena block, so its size is the footprint of every collection pass and of a retained heap. Keep cold state out of line rather than raising the cap: `callable` is `Option<Box<JsFunction>>` (the per-function bytecode verdict lives in `JsFunction::User`, read through `bytecode_cache()`), private elements / instance field definitions / native GC roots share one lazily allocated `extras: Option<Box<ObjectExtras>>` (use `private_field`, `instance_field_defs`, `gc_native_roots` and their `_mut` forms), and every `ObjectKind` payload over 32 bytes is boxed (`Iterator`, `Promise`, `Map`, `Set`, `TypedArray`, ...; assign generator state with `set_iterator_state`, which reuses the box). `PropertyMap` keeps `INLINE_CAP = 3` entries inline, a fourth spills to a right-sized heap buffer, and it stores `PropertyRecord`s (32 bytes: three `JsValue` slots plus a presence-flag word, the same information as a 56-byte `PropertyDescriptor`); `get`/`entry_at`/`iter` hand out `&PropertyRecord` (read through `value()`, `getter()`, `writable()`, ...; mutate through `set_value`, `set_writable`, ...) and `to_descriptor()` materialises a `PropertyDescriptor`, while `insert`/`push_fresh`/`remove` still take and return descriptors.
- The major collection is two passes after the mark: an ephemeron fixpoint over only the WeakMaps the mark reached (`trace_mark_worklist` collects them), then one sweep-and-tenure pass. `reset_generations_after_major` runs before that pass so its `remember_if_old` calls are kept. `free_gc_object` skips side-table removes on empty tables.
- `then`/`catch`/`finally`-derived promises, `Promise.resolve`/`reject` results and async-function result promises built from the running realm's `%Promise%` never materialise resolve/reject function objects: a derived promise is a `ReactionTarget::Internal(promise_id)` that the reaction job settles through `resolve_promise_internal`/`reject_promise`, and an async result promise is `PromiseResolvers::Internal` (guarded by `PromiseData.already_resolved`). Exposed pairs (`create_resolving_functions`) remain for `new Promise`, `withResolvers`, the combinators and subclass/module machinery (`ReactionTarget::Capability`, `PromiseResolvers::Functions`). `%Promise%`, `%Promise.prototype.then%` and the original `Promise[@@species]` getter are realm intrinsics (`Realm.promise_constructor`/`promise_then`/`promise_species_getter`) — never test the `Promise` global binding, which user code can reassign. `SpeciesConstructor` lookups stay observable; only the call of the original species getter is skipped. The thenable job for a native promise with the original `then` registers a handler-less `Internal` reaction on the thenable instead of calling `then`.
- The microtask queue holds `scheduler::Microtask` values: typed `Reaction`/`ResolveThenable` jobs for the Promise machinery (their GC roots are read straight off the job by `Microtask::for_each_root`, no per-job roots `Vec` or boxed closure) and a `Closure { roots, job }` form for one-off producers. `run_microtask` roots a dequeued job for its duration and is the single place the three drain loops run one. An `await` continuation is `ReactionTarget::AsyncResume(async_id)` with no handler: the reaction job calls `async_function_resume` directly, so awaiting creates no function objects and an await of a non-object queues one job with no promise (`async_fn_suspend_at_await`). `AsyncFunctionState` stays in the scheduler map for the whole resume so the collector keeps tracing it; the driver works on clones of the pieces it mutates and moves its stacks back into the entry at the next suspension. It records the function's `realm_id`, which `async_function_resume` installs around every resumption (a job otherwise runs in the realm that drained the queue). Async generators follow the same shape (#1061): a request's promise is settled through `PromiseResolvers::Internal` (`async_gen_resolve_request`/`async_gen_reject_request`; `AsyncGenRequest` carries no resolving functions), and its `await`, `yield` and await-return continuations are `ReactionTarget::AsyncGen(gen_id, AsyncGenStep)` reactions with no handler, which recover the request promise from the front of `async_gen_queue(gen_id)` (a request is popped only after it settles, and a non-empty queue is what roots the generator). `yield*` delegation and disposal still use `await_then` closures.
- RegExp `exec` runs from an `ExecPlan` (decoded pattern, flag bits, `Rc<CachedRegex>`, plus the pattern text and its occupied stand-in windows for the per-subject window shift described below) memoized on `RegExpData.exec_plan` on first use; `compile()` replaces the whole `RegExpData`, so the plan can never go stale, and `regexp_exec_raw` reads it only after `ToLength(lastIndex)`. A sticky match is run anchored at `lastIndex` (`regex_captures_at(.., anchored)`: fancy-regex `RegexInput::anchored`, `regex_automata::meta` `Anchored::Yes`; the plain and WTF-8 matchers are built through the meta engine with the `regex` crate's defaults because `regex::Regex` cannot anchor at an offset), so a failed attempt costs O(1) in the remaining subject rather than a forward scan. Converted subjects are cached in `SubjectCache` (4-entry LRU for subjects of 256+ code units, one slot for shorter ones) so `.test(short)` between execs of one long subject does not evict it (#921, `docs/perf/2026-10-12/921-regexp-sticky-anchored.md`).
- `new.target` is the `[[NewTarget]]` of the function environment, stored beside `this` in `ThisBinding.new_target` and resolved by the same chain walk (`Environment::get_new_target`): arrows, direct eval, field initializers (undefined) and resumed async/generator bodies need no capture or save/restore. `Interpreter::new_target` is only the hand-off to a *native* constructor (`apply_new_target_prototype`, `get_prototype_from_new_target_realm`), set by `call_constructor_body` and cleared while a user body runs.
- RegExp matching runs over a Rust `String` view of the UTF-16 subject, so a lone surrogate is a *stand-in* scalar from a 2048-scalar window of U+F0000..U+10FFFF (`STAND_IN_WINDOW`, `regexp.rs`). In Unicode mode a genuine scalar of that range would be indistinguishable from a stand-in, so each regex is translated for the first window that neither the pattern text nor the subject occupies (`pattern_window_mask`/`subject_window_mask`, `ExecPlan::cached_for`, cached per `(base, flags, source)`); class ranges carve that window out (`append_class_range_at`), and lone surrogates in a Unicode pattern are escaped as `\u{XXXX}` for the translator (`js_string_to_unicode_pattern`). Every match path takes its regex from `ExecPlan::cached_for(interp, input)`, never `plan.cached` directly. When all 64 windows are occupied the translation uses `BYTES_MODE_BASE` (the default window, no carving, and `\p{Cs}`/`\p{Co}` over WTF-8 bytes), which is not exact once a lone surrogate is involved (a genuine scalar in each of the 64 windows, as few as 64 characters, makes a pattern or subject lone surrogate indistinguishable from the genuine scalar at its default-window position; an exact fix needs a WTF-8 byte-level alternation for every construct that can match a lone surrogate and cannot cover fancy-regex or the custom lookbehind matcher, #1009). Non-Unicode matching sees one scalar per code unit, so the default window always suffices. Regular expression literals reach the AST as UTF-16 code units (`Literal::RegExp(Vec<u16>, ..)`), decoded from `encode_source_units` text by the lexer, never as stand-in text.
- The `Object()` constructor calls `to_object()` to wrap primitives (String, Number, Boolean, Symbol, BigInt).
- Strings are capped at `MAX_STRING_LENGTH` (`types.rs`, 2^29 - 24 code units, V8's limit); string-building paths throw `RangeError: Invalid string length` instead of allocating. Code that accumulates into a `Vec<u16>` uses `append_code_units`/`append_js_string` (checked), the `+` path uses `concat_string_values` (returns `None` when too long), and `padStart`/`padEnd`/`repeat`/`concat`/template literals/case mapping check explicitly. `replace`/`replaceAll`/`RegExp.prototype[Symbol.replace]` and `get_substitution` append through `append_code_units`, so an oversized replacement result throws before it is built. Tests that need strings near the cap live in `tests/string-max-length.js` and `tests/string-max-length-replace.js` (the test262 runner's 512 MB limit cannot hold them; they peak at about 2.6 GiB and 1.5 GiB).

## Agent skills

### Issue tracker

Issues live on GitHub at `pmatos/jsse` and are managed via the `gh` CLI; external PRs are not a triage surface (issues only). See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical triage roles map to the tracker labels `needs-triage`, `needs-info`, `agent-ready`, `human-ready`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout — one `CONTEXT.md` + `docs/adr/` at the repo root (created lazily by `/domain-modeling`). See `docs/agents/domain.md`.
