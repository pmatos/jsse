# Execution Counters (performance investigations)
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
- `gc_env_probes` / `gc_env_first_visits` count environment-walk entries during collections (`EnvVisits::first_visit`) and the subset that were the first visit of their collection; the rest stopped a parent-chain walk at an already-stamped environment. `gc_env_clean_skips` counts minor-collection walk entries that skipped a clean environment.
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
