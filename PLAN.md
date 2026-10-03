# Plan: issue #808 — bytecode VM safepoints beyond loop back-edges

## 1. Problem restated

`src/interpreter/bytecode/vm.rs`'s dispatch loop calls `interp.gc_safepoint()`
from exactly one place: the negative-offset arm of `Op::Jump` (loop
back-edges, `vm.rs:653-661`). The tree-walker, by contrast, safepoints in two
places: once per loop iteration (same back-edge timing, e.g.
`exec_while`/`exec_for` in `exec.rs`) *and* once before executing each
statement in a statement list (`exec_prepared_statements`, `exec_eval_body`
in `exec.rs`). A compiled chunk that contains no `while`/`for` loop — any
straight-line sequence of statements, including an entire compiled script
body — currently executes zero safepoints no matter how much it allocates.
Under `JSSE_GC_STRESS`, that means `--bytecode` runs exercise a collector
at a tiny fraction of the points the tree-walker does, so missing-root bugs
in loop-free code have almost no chance of being caught by stress mode when
the bytecode VM is enabled. The fix is to give the bytecode VM a second
safepoint source — one per statement-list position — matching the
tree-walker's existing granularity, without changing the loop back-edge
safepoint that already exists and already matches the tree-walker.

## 2. Spec basis

N/A: no JavaScript behavior change. This only changes how often the engine's
existing mark-and-sweep collector runs while interpreting already-compiled
bytecode; it changes no evaluation order, no produced value, and no observed
throw behavior. Collection cadence is not observable from JS except through
implementation-defined/host-defined hooks (test262's `$262.gc()` host
extension, and the WeakRef/FinalizationRegistry cleanup-callback timing that
ECMA-262 deliberately leaves implementation-defined) — none of which this
change touches; it only makes more of the *existing* collector's scheduling
decisions fire at points they were previously unable to reach.

## 3. Files to touch

- `src/interpreter/bytecode/op.rs` — add `Op::Safepoint = 56` to the enum and
  to `from_u8`.
- `src/interpreter/bytecode/compiler.rs` — emit `Op::Safepoint` immediately
  before compiling each statement at every *statement-list* position:
  `compile_body`'s loop, `compile_script_body`'s loop, and
  `Statement::Block`'s loop inside `compile_statement`. Add a
  `debug_assert_eq!(self.current_stack, 0); debug_assert_eq!(self.current_refs, 0);`
  immediately before each emission, mirroring the existing asserts at the
  `While`/`For` back-edges (`compiler.rs:627-628`, `:662-663`).
- `src/interpreter/bytecode/vm.rs` — add the `Op::Safepoint` dispatch arm:
  repeat the two `debug_assert!`s already used at the back-edge (`stack` and
  `refs` empty) and call `interp.gc_safepoint()`. No operand bytes, so no
  extra `pc` advance beyond the generic `pc += 1`.
- `src/interpreter/perf_counters.rs` — update the compile-time bound check
  (`perf_counters.rs:33`, currently asserting `Op::Construct` is the highest
  opcode `< OP_SLOTS`) to cover `Op::Safepoint` instead (still well under
  `OP_SLOTS = 64`, no resize needed). In `record_op`, count `Op::Safepoint`
  into `vm_op_hist` (so it's visible in the `OP` table) but **not** into
  `vm_ops` — the tree-walker's equivalent per-statement `gc_safepoint()`
  call is not counted in `ast_stmts`/`ast_exprs` either, and CLAUDE.md's
  "Execution Counters" contract depends on the compiled/tree-walker `OP`
  vs. AST-unit totals staying comparable (#524's published split). Counting
  the new opcode into `vm_ops` would inflate bytecode "work" relative to the
  tree-walker for no semantic reason.
- `src/interpreter/gc.rs` — add a `#[cfg(test)] pub(crate) fn stress_hits(&self) -> u64 { self.stress_count }`
  getter on `GcPacer`, alongside the existing `#[cfg(test)] set_stress_period`.
  Test-only, zero production cost.
- `src/interpreter/bytecode/tests.rs` — new unit tests (see §4).
- `tests/gc_stress.rs` — extend with a `--bytecode` variant (see §4).
- `CLAUDE.md` (and its `AGENTS.md` symlink, so one edit covers both) — the
  "GC Stress Mode" bullet currently reads: *"the bytecode VM only has
  back-edge safepoints, so `--bytecode` stress is sparser"* (line 81). Update
  it to describe the new statement-boundary safepoints and that `--bytecode`
  stress coverage is now close to tree-walker parity for compiled chunks
  (still gated by `CompileGoal`/`CompileError::Unsupported` bail coverage —
  see §6).
- No `docs/adr/` entry: this is a mechanical extension of an existing,
  already-documented mechanism (same `gc_safepoint()`, same debug-assert
  discipline, same placement convention already used by the tree-walker),
  not a new architectural decision. The existing design docs
  (`docs/specs/2026-07-20-bytecode-loop-slice-design.md`,
  `docs/specs/2026-07-25-bytecode-member-access-slice.md`) already describe
  the invariants this change relies on (operand stack empty at statement
  boundaries, continuous per-value rooting via `gc_bytecode_roots`); no spec
  doc edit is required for a mechanical slice, but the implementer should
  skim both before touching `vm.rs` since they're the authoritative
  rationale for the rooting discipline the new opcode must not violate.

## 4. TDD slices

1. **Red:** in `src/interpreter/bytecode/tests.rs`, add
   `straight_line_chunk_gets_zero_safepoints_today` (or similar) that: builds
   an `Interpreter`, calls `interp.gc.set_stress_period(1)`, compiles a
   loop-free, multi-statement function body (e.g. three `var` declarations
   each initialized from a fresh object literal, no `while`/`for`) via
   `compile_body`, runs it with `run_chunk`, and asserts
   `interp.gc.stress_hits() == <statement count>`. This fails today at
   `stress_hits() == 0` (no safepoint exists anywhere in the chunk).
   **Green:** add `Op::Safepoint` (op.rs), emit it before each statement in
   `compile_body`'s loop (compiler.rs), add the VM dispatch arm (vm.rs). This
   slice alone makes the test pass for top-level function bodies.
2. **Red→Green, same pattern, `compile_script_body`:** add a script-body
   variant of the same test (reusing `eval_script_completion_with_mode`'s
   pattern already in `tests.rs`) proving a straight-line *script* (not just
   a function) now safepoints per statement. Emit `Op::Safepoint` in
   `compile_script_body`'s loop.
3. **Red→Green, nested `Statement::Block`:** add a test with a function body
   containing an explicit `{ ... }` block with multiple statements (no
   loop), asserting the inner block's statements each get their own
   safepoint too (statement count across both nesting levels). Emit
   `Op::Safepoint` in `Statement::Block`'s loop in `compile_statement`.
4. **Green (no new red needed — existing invariant):** add
   `single_statement_loop_body_keeps_one_safepoint_per_iteration`: compile a
   `while`/`for` whose body is a *single* non-block statement (e.g.
   `for (var i = 0; i < 3; i++) sink = i;`), run it with
   `stress_period(1)`, and assert `stress_hits()` equals the iteration count
   — *not* double that. This locks in the deliberate scope decision (§6):
   loop bodies that aren't blocks get no additional statement-boundary
   safepoint beyond the existing back-edge one, exactly matching
   `exec_while`/`exec_for`'s tree-walker behavior (verified directly against
   `exec.rs:1079-1097`, `1804-1825`: the tree-walker's `If`/`While`/`For`
   arms never safepoint around a single-statement body, only around
   statement-*list* entries and loop iterations).
5. **perf_counters green:** extend the existing `record_op`/`OP` table unit
   tests in `perf_counters.rs` with a case exercising `Op::Safepoint`,
   asserting it appears in `vm_op_hist` / the `OP` table but is excluded from
   `vm_ops`'s denominator.
6. **Integration green:** extend `tests/gc_stress.rs` with a `--bytecode`
   invocation of the binary (`cmd.args(["--bytecode", "-e", PROGRAM])`) using
   a *loop-free* program (new `const`, since the existing `PROGRAM` is
   loop-heavy and already gets back-edge coverage) at
   `JSSE_GC_STRESS` periods `1`, `2`, `7`, asserting identical output to the
   unstressed baseline — this is the end-to-end proof that the issue's
   literal premise ("`--bytecode` stress barely covers straight-line code")
   is closed.

## 5. Test surface

- `cargo test --release` — all new unit tests (slices 1-5) and the extended
  `tests/gc_stress.rs` integration test (slice 6).
- `cargo build --profile release-checked` +
  `uv run python scripts/run-test262.py test262-extra/ --binary target/release-checked/jsse --bytecode`
  and the same with `--bytecode` omitted — confirms the new statement/refs
  empty-stack debug_asserts never fire (i.e. the compile-time invariant this
  change leans on — `current_stack`/`current_refs == 0` at every emission
  site — actually holds for every construct the bytecode compiler accepts,
  not just the ones exercised by the new unit tests).
- `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/ --bytecode --binary target/release-checked/jsse --timeout 300`
  — the direct regression gate for this issue: forces a collection at (close
  to) every statement boundary under `--bytecode` and should surface any
  latent missing-root bug that was previously unreachable because no
  safepoint existed to trigger it there (see §6).
- A sampled full `test262` run with `--bytecode` plus `JSSE_GC_STRESS` in the
  `N=16..1000` range (per the existing GC Stress Mode guidance), on the
  default release binary — broader cross-section, accepting the
  non-determinism the existing doc already calls out for sampled stress
  runs. Never `--update-baseline`.
- No new `test262-extra/` or `tests/` file is needed to cover a *spec*
  behavior — this change has none (§2) — but the GC-correctness regression
  surface (§6) is real and is covered by the stress run above plus the new
  unit/integration tests in §4, not by test262 content.
- Perf validation (not pass/fail against a suite, but a required check
  before calling this done): run `target/release/jsse --bytecode benchmarks/scripts/bench_opmix.js`
  before and after, several times each, and compare wall-clock output
  against the existing baseline recorded in
  `docs/perf/2026-09-25/engine-comparison.json`
  (`jsse-v0.9.0-bytecode/bench_opmix: 1.82s` vs. tree-walker `3.08s`). The
  `arith` loop body in that benchmark is a 6-statement block, so this change
  is *expected* to add up to 6 new (off-path, single-branch) safepoint calls
  per loop iteration there — bringing bytecode's per-statement cost toward,
  not past, the tree-walker's existing cost for the same shape. Acceptance:
  `--bytecode` must stay faster than the tree-walker baseline on
  `bench_opmix`; it is explicitly not required to stay at today's
  `--bytecode` number, since today's number is partly an artifact of the gap
  this issue closes.

## 6. Regression risk

- **Baseline (`test262-pass.txt`):** should not move. `--bytecode` is off by
  default, the default test262 run (no `--bytecode`, no `JSSE_GC_STRESS`)
  takes a code path this change does not touch, and `compile_statement`'s
  accepted-statement set (`Empty`, `Expression`, `Block`, `Variable`, `If`,
  `While`, `For`, `Return`) and its `CompileError::Unsupported` bail
  behavior are unchanged — this change only adds an opcode that fires
  *inside* chunks the compiler already accepts, it adds no new acceptance
  and removes no fallback.
- **Latent missing-root bugs under `--bytecode` + `JSSE_GC_STRESS`.** This is
  the actual point of the issue, but also the actual risk: these are new
  *production* collection points (not just stress-only), so they can surface
  a value that was only ever safe because no collector run happened to land
  between its allocation and its use — e.g. a call-IC's cached
  `callee_obj_id`/shape outliving an id recycle, or an `env`/`this` binding
  the VM assumed stayed implicitly alive across a statement boundary. The
  §5 `release-checked` + `JSSE_GC_STRESS` test262-extra run and the sampled
  full-suite run are the gates for this; triage per the existing CLAUDE.md
  "GC Stress Mode" recipe (minimal repro + `$262.gc()`, diff against the
  same run without the variable).
- **`refs` (the VM's `ResolveName`/`StoreResolvedName` reference stack) is
  not in `collect_gc_roots` at all** — it holds `IdentifierRef`, not
  `JsValue`, so it has nothing to root, but the Op::Jump back-edge's existing
  `debug_assert!(refs.is_empty(), ...)` is the actual safety argument for
  *that* safepoint, and the new statement-boundary safepoints reuse the same
  assert. Compiler bookkeeping (`push_ref`/`pop_ref`) already guarantees
  `current_refs == 0` at every statement boundary (refs are always paired
  within one compiled construct), so this is a restated invariant, not a new
  one — but it is the thing to re-verify first if a stress run finds a panic
  here instead of a silently-wrong value.
- **`#807` (open, "refactor(gc): unify or assert gc_bytecode_roots, the
  third root stack")** is in flight on a sibling branch and retypes
  `gc_bytecode_roots`/`unroot_stack_value` and adds a chunk-exit balance
  assert in `run_chunk_with_var_prologue`. The new `Op::Safepoint` arm only
  ever fires when the compiler has proven `stack`/`refs` (and therefore this
  chunk's slice of `gc_bytecode_roots`) are empty, so it does not add any
  new entries to `gc_bytecode_roots` and should not interact with #807's
  balance assert either way — but both changes touch `vm.rs` in the same
  region, so expect a textual (not semantic) rebase conflict whichever lands
  second. Not a blocking dependency; worth one line in the PR description.
- **Perf:** the `benchmarks/scripts/bench_opmix.js` `arith`/`mixed` cases are
  the known-sensitive benchmark (see §5) — any regression beyond "still
  faster than tree-walker" should be investigated (e.g. consider skipping
  the statement-boundary safepoint for the loop body's *first* statement
  when the loop already has a back-edge safepoint immediately preceding it,
  if measurement shows this matters) rather than silently accepted.
- **Shared machinery exercised:** `gc_safepoint()`/`GcPacer` (gc.rs,
  unchanged logic, just more call sites), the bytecode compiler's
  stack-depth tracking (`current_stack`/`current_refs`, compiler.rs,
  unchanged logic, new call sites only), and the perf-counters `OP`
  histogram contract (perf_counters.rs, see §3). Not touched: the
  tree-walker hot paths, `property.rs`'s MOP, any `ObjectKind` match, any
  Node-compat library harness.

## 7. Out of scope

- **Safepoints on `Op::Call`/`Op::ReturnCall`/`Op::Construct` themselves.**
  These already rely on the callee reaching its own safepoints recursively
  (explicit comments at `vm.rs:506-509`, `:558-561`) and on keeping their own
  operands continuously rooted via `gc_bytecode_roots` across the nested
  invocation. That is a call-boundary design question, not the
  statement-boundary gap #808 is about; leave it for a separate issue if a
  gap is ever demonstrated there (e.g. a native builtin that allocates
  heavily with no JS-level statement boundary to hook).
- **Widening the compiler's accepted-statement surface** —
  `CompileError::Unsupported` still bails to the tree-walker for `DoWhile`,
  `ForIn`, `ForOf`, `Try`, `Switch`, `Labeled`, `With`, generators, classes,
  etc. Unaffected by this change and out of scope here.
- **Unifying `gc_bytecode_roots` onto `RootStack`** — that is #807's scope,
  not this issue's; see the coordination note in §6.
- **Any rebalancing of the GC pacer's allocation-pressure heuristics**
  (`charge_object`, thresholds, major/minor suppression logic) — unrelated
  to safepoint *placement*, not touched.
- **Rewriting `benchmarks/run_benchmarks.sh`** to add `--bytecode`/
  `bench_opmix` support — useful but a separate tooling cleanup; the perf
  check in §5 uses a direct invocation instead.
- **Adding `JSSE_GC_STRESS` + `--bytecode` to CI** (`.github/workflows/ci.yml`)
  — valuable given the gap the research surfaced (CI currently never sets
  `JSSE_GC_STRESS` at all), but a CI-workflow change is a separate, reviewable
  unit from the VM/compiler change itself; flag it as a follow-up rather than
  bundling it into this PR.
