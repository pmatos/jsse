# Plan: issue #806 — convert remaining multi-exit manual root frames to `with_gc_root_scope`

## 1. Problem restated

Six native/interpreter functions still pair raw `gc_root_frame()`/`gc_unroot_frame(frame)`
calls by hand across every exit path instead of using the `with_gc_root_scope(|interp| ..)`
closure combinator (`src/interpreter/mod.rs:1490`) that bulk-truncates the GC temp-root
stack on every exit automatically: `Object.fromEntries`, `Array.from`'s iterator path (two
frames held simultaneously), the `super()` call branch of `eval_call`, `construct_from_evaluated`,
`eval_assign`, and `call_async_function`. This is pure internal bookkeeping cleanup — today's
code is already root-balanced (verified below site by site) — done for the same reason #595
and the `gc-root-scope-guard-eval` slice were: fewer hand-threaded teardown calls means fewer
places a future edit can silently unbalance the stack. Two of the six (`eval_call`'s hot
dispatch path and `eval_assign`) are named in the issue as needing a perf check before
conversion, since they run on every call/assignment expression. The ADR
(`docs/adr/2026-09-10-2014-gc-root-scope-guard.md`) currently claims `Array.from`'s nested
frames can't be expressed with the single-frame combinator; this plan's slice 4 revisits
that claim with a nested-`with_gc_root_scope` construction and amends the ADR either way.

## 2. Spec basis

N/A: no JavaScript behavior change. GC temp-root bookkeeping is an internal memory-safety
mechanism with no spec-observable semantics — ECMAScript does not mandate a garbage
collection strategy. Each slice below must produce byte-identical `Completion` values and
side effects on every exit path the original hand-written frame covered; that invariant is
checked by the existing test262 suite, the release-checked build's `gc_assert_root_depth`
debug-asserts, and `JSSE_GC_STRESS`, not by a spec clause.

## 3. Files to touch

- `src/interpreter/builtins/mod.rs` — `Object.fromEntries` native (~6515-6591).
- `src/interpreter/builtins/array.rs` — `Array.from`'s iterator-protocol branch (~2961-3049).
- `src/interpreter/eval.rs` — the `super()` branch inside `eval_call` (~4536-4599),
  `construct_from_evaluated` (~6433-6838), the `Expression::Member` assignment branch of
  `eval_assign` (~2772-3133), `call_async_function` (~7875-8061).
- `src/interpreter/tests.rs` — one new unit test for `construct_from_evaluated` (slice 2).
- `docs/adr/2026-09-10-2014-gc-root-scope-guard.md` — amend in place: retract the "`Array.from`'s
  nested frames ... correctly keep the raw primitive" claim, record the nested-scope pattern,
  and append this PR's conversions to "Applied so far" (the same append style the `yield*`
  slice used — "this change" paragraph — not a rewrite of prior entries' reasoning).
- No `test262/` or `spec/` changes (forbidden; also not needed — no behavior changes).
- No new `test262-extra/` or `tests/` files — see §5 for why the existing suite is the oracle.

## 4. TDD slices

Five of these six sites are behavior-preserving refactors with no new JS-observable outcome,
so there is no literal red test to write for them — the "red" state is "the hand-written
teardown is correct but fragile," and "green" is "`with_gc_root_scope` reproduces every exit
identically, confirmed by rerunning the same oracles before and after." Each slice is its own
commit. Order front-loads the lowest-risk, non-hot-path sites and the one slice that *does*
get a real regression test, and defers the two perf-gated hot-path sites to the end so the
PR can still close most of the issue if a gate rejects one of them.

1. **`Object.fromEntries`** (`src/interpreter/builtins/mod.rs:6515-6591`).
   One frame (`gc_frame` at 6527), 7 unroot sites (6534, 6541, 6547, 6556, 6566, 6576, 6585),
   all straight-line, no `gc_unroot_value` mixing, fully synchronous (no await/yield inside).
   Wrap the closure body in `interp.with_gc_root_scope(|interp| { .. })`, delete the manual
   frame/unroot lines, turn each `interp.gc_unroot_frame(gc_frame); return Completion::Throw(e)`
   into a plain `return Completion::Throw(e)`.
   Oracle: `cargo test`; `uv run python scripts/run-test262.py test262/test/built-ins/Object/fromEntries/`
   on the default release build and again on `target/release-checked/jsse`; `JSSE_GC_STRESS=1`
   over the same directory (N=1 is affordable — the directory is small).

2. **`construct_from_evaluated`** (`src/interpreter/eval.rs:6433-6838`).
   One frame (`gc_frame` at 6439, rooting `callee_val` and `evaluated_args`), 7 unroot sites
   (6477, 6484, 6499, 6531, 6578, 6608, 6827). The field-initializer passes (6702-6819) also
   contain 10 `return` statements (6708, 6713, 6725, 6730, 6760, 6767, 6772, 6790, 6799, 6806)
   that bypass the unroot — **confirmed not a live leak**: both of this function's callers
   unconditionally release their own operands after the call regardless of outcome —
   `eval_new` (`eval.rs:6406`/`6420`, tree-walker `new`) and the bytecode VM's `Op::Construct`
   handler (`src/interpreter/bytecode/vm.rs:552-573`, specifically the `release_construct_operands`
   call at 563) — so whatever the naked returns leave rooted is swept up before the next
   `gc_assert_root_depth` checkpoint either way. This matches the issue's "balanced today"
   framing; converting tightens the rooting (truncates at this function's own exit instead of
   relying on an ancestor) without fixing a live bug, and the PR/commit message should say so
   plainly rather than claim a bug fix.
   Wrap the whole body in `with_gc_root_scope`, turn all 7 explicit unroot+return pairs and
   the 10 naked returns into plain `return`s.
   New test (the one real red/green step in this plan): in `src/interpreter/tests.rs`, next to
   `with_gc_root_scope_truncates_on_every_exit` (~line 5410), add a test that evaluates a small
   class whose public field initializer throws (e.g. `class C { x = (() => { throw 1 })(); }`),
   calls `interp.construct_from_evaluated(&callee_val, &[], &env)` directly (it's `pub(crate)`),
   and asserts `interp.gc_root_frame()` is back to the pre-call depth immediately after the
   call returns — independent of any ancestor frame. Write this test against the
   pre-conversion code first to confirm it fails (the naked-return path leaves the frame open
   at the point this test checks, even though no outer boundary currently catches it), then
   make the conversion and confirm it passes.
   Oracle: the new unit test, plus `cargo test`; `uv run python scripts/run-test262.py
   test262/test/language/statements/class/ test262/test/built-ins/Reflect/construct/` on the
   default release build and on `target/release-checked/jsse` (both with and without
   `--bytecode`, since this function has the VM caller too); `JSSE_GC_STRESS=16 --sample 0.2
   --seed <fixed>` over `test262/test/language/statements/class/`.

3. **`call_async_function`** (`src/interpreter/eval.rs:7875-8061`).
   One frame (`gc_frame` at 7889), 2 unroot sites (7987 — `bind_function_parameters` error,
   8052 — normal path). Single caller (`eval.rs:5250`), not reachable from the bytecode VM by
   name. The scope must still be open when `self.async_function_resume(async_id, ..)` is
   called at line 8050 — that call kicks off the function's first synchronous resumption
   before any `await` suspends it — so the `with_gc_root_scope` wrap must close at the same
   point the manual frame did (line 8052), not earlier. `promise`/`resolve_fn`/`reject_fn` are
   moved into `AsyncFunctionState` and traced independently by the scheduler
   (`src/interpreter/gc.rs:510-513`) well before the scope closes, so nothing here needs a
   Pinned Native Root — this is a single synchronous kickoff call, not a multi-tick body.
   Wrap the body through line 8052 in `with_gc_root_scope`; the two post-scope `return`s at
   8057-8059 are unaffected (they run after the scope already closed, matching the documented
   "tail value isn't re-rooted" behavior).
   Oracle: `cargo test`; `uv run python scripts/run-test262.py
   test262/test/language/statements/async-function/ test262/test/built-ins/AsyncFunction/` on
   default release and `release-checked` builds; `JSSE_GC_STRESS=1` over the same two
   directories.

4. **`Array.from`'s iterator path** (`src/interpreter/builtins/array.rs:2961-3049`).
   The array-like `else` branch (3052-3104) is already migrated (ADR, PR #595); only the
   `if let Some(iter_method) = using_iterator` branch remains. Today it holds two frames open
   at once: an outer `gc_frame` (2972, spanning the whole loop, rooting `a` and `iterator`)
   and an inner `gc_frame_next` (3007, opened and closed every iteration, rooting `next`).
   The ADR's "the combinator cannot express two frames alive at once" claim is about a
   *single* `with_gc_root_scope` call; it does not rule out nesting one inside another, which
   the ADR itself says composes fine for plain LIFO cases. Convert as:
   - Outer: `interp.with_gc_root_scope(|interp| { .. })` wrapping from where `a` is rooted
     through the whole `loop { .. }`. Every `return` directly inside this closure's body
     (including inside the `loop`, but outside the inner closure) is a `return` from this
     closure — Rust resolves `return` to the nearest enclosing closure/fn, and this loop lives
     directly in the outer closure's body — so it triggers the outer scope's truncation
     exactly like the manual `gc_unroot_frame(gc_frame); return ..` pairs did.
   - Inner: each iteration's `gc_frame_next` region becomes a nested
     `interp.with_gc_root_scope(|interp| -> IterStep { .. })` call (a small local enum,
     e.g. `enum IterStep { Done, Value(JsValue), Abrupt(Completion) }`, is clearer than
     `Option<Completion>` once the "iterator exhausted" case is included) that roots `next`,
     computes the per-iteration value (or the abrupt completion from `iterator_step`/
     `iterator_value`/the map function/`iterator_close`), and returns one of the three
     variants. The outer loop matches on it: `Abrupt(c) => return c`, `Done => { set_length,
     return Completion::Normal(a) }`, `Value(v) => { create_data_property_or_throw(..); k += 1
     }` (an error from `create_data_property_or_throw` stays a direct `return` from the outer
     closure, same as today).
   - This is behavior-equivalent: today, an abrupt exit inside one iteration truncates
     straight to `gc_frame`'s depth (below where `gc_frame_next` was opened); with nesting,
     the inner `with_gc_root_scope` truncates to its own (deeper) depth first and then the
     outer's `return` truncates to the same final depth — same end state, same per-iteration
     release of `next` on the non-abrupt path (matching today's `gc_unroot_frame(gc_frame_next)`
     at 3047, so no root-stack growth across iterations).
   After this conversion, amend the ADR (see §3) to record the nested pattern and narrow the
   "raw primitive reserved for" list to the cases that still need it: driver loops
   (`drain_microtasks`/`run_due_timers`/`await_value`), the bytecode VM's
   `root_operand_stack`, and the two sites that unroot by value identity rather than frame
   truncation (`exec.rs`'s `bind_pattern`, `eval.rs`'s `destructure_array_assignment`).
   Oracle: `cargo test`; `uv run python scripts/run-test262.py test262/test/built-ins/Array/from/`
   on default release and `release-checked` builds (with and without `--bytecode`, since
   compiled code can call this native); `JSSE_GC_STRESS=1` over the same directory (small
   enough for N=1); additionally `./scripts/run-library-tests.sh acorn` and
   `./scripts/run-library-tests.sh decimal.js` as a cheap real-world sanity net, since both
   are green today and acorn/decimal.js code exercises `Array.from` with real iterables.

5. **`super()` call branch** (`src/interpreter/eval.rs:4536-4599`, inside `eval_call`,
   whose own span is 4525-4893 with two unrelated frame regions of its own at 4789-4799 and
   4804-4891 that this slice does not touch). One frame (`gc_frame` at 4562), 3 unroot sites
   (4566, 4577, 4591); the `return Completion::Throw(e)` at 4581 runs *after* the frame is
   already closed at 4577, so it is not a hidden leak. Wrap the branch body (4562-4598) in
   `with_gc_root_scope`. **Perf-gated**: this branch only executes for an actual `super(...)`
   call (a narrow subset of all calls through `eval_call`), so the practical exposure is much
   smaller than `eval_assign`'s, but the issue names `eval_call` as a hot path, so measure
   before committing. Method: snapshot a release build of the pre-slice commit to
   `$TMPDIR/jsse-before` (uninstrumented `--release`, capped `-j` per the autonomous-run
   budget), build the post-slice binary to `$TMPDIR/jsse-after`, then interleave ≥10 runs each
   of a tight loop constructing a derived class (`class B extends A { constructor() { super();
   } }`, instantiated in a loop) plus the existing `benchmarks/scripts/bench_opmix.js`, and
   compare medians. If the post-slice median regresses beyond run-to-run noise, drop this
   slice from the PR (leave the manual frame, note the measured numbers and the decision in
   the PR description and as a follow-up issue) rather than force it through.
   Oracle (if kept): `cargo test`; `uv run python scripts/run-test262.py
   test262/test/language/expressions/super/ test262/test/language/statements/class/` on
   default release and `release-checked` builds; `JSSE_GC_STRESS=1` over
   `test262/test/language/expressions/super/`.

6. **`eval_assign`** (`src/interpreter/eval.rs:2622-3190`). Only one frame in the whole
   function, scoped to the `Expression::Member` branch (2772-3133): `gc_frame` at 2858,
   already wrapped in a hand-rolled IIFE (`let result = (|| { .. ~22 internal returns .. })();
   self.gc_unroot_frame(gc_frame); result`, closing at 3131) — the IIFE already *is* the
   `with_gc_root_scope` shape with a different spelling. Conversion is a mechanical
   `(|| { .. })()` → `self.with_gc_root_scope(|interp| { .. })` swap, renaming `self` to
   `interp` inside (~270 lines) and deleting the two manual frame lines. **Perf-gated**: this
   runs on every assignment expression in every script, the hottest of the six sites. Because
   the current code is already an equivalent closure, the expected overhead delta is close to
   zero, but measure rather than assume. Same method as slice 5 (before/after release
   binaries, capped `-j`, ≥10 interleaved runs), using `benchmarks/scripts/bench_fib.js`,
   `bench_closures.js`, `bench_loop.js`, `bench_object.js`, and `bench_opmix.js` (all
   assignment-heavy). Same explicit drop rule: if the median regresses beyond noise, keep the
   IIFE, record the measurement, and file a follow-up issue instead of forcing the conversion.
   Oracle (if kept): `cargo test`; `uv run python scripts/run-test262.py
   test262/test/language/expressions/assignment/ test262/test/language/expressions/compound-assignment/`
   on default release and `release-checked` builds (with and without `--bytecode`); `JSSE_GC_STRESS=16
   --sample 0.3 --seed <fixed>` over both directories (larger directories, so N=1 is not
   affordable; a fixed seed makes the run reproducible for a follow-up debugging session if it
   fails).

**After all slices land** (whichever of 5/6 survive their perf gate): run the full suite on
the default release build (`cargo build --release`; `uv run python scripts/run-test262.py`)
and diff against the baseline — it must be byte-identical, no new passes or failures (`test262-pass.txt`
is read from `origin/main`; do not pass `--update-baseline`). Also run
`cargo build --profile release-checked` and `uv run python scripts/run-test262.py --binary
target/release-checked/jsse test262-extra/` both with and without `--bytecode`, plus a
`--sample 0.1` run of the full suite on the same binary — the same three gates CI already
runs for this kind of change. Finish with `./scripts/lint.sh`.

## 5. Test surface

- **Targeted test262, per slice**: listed under each slice in §4 — `built-ins/Object/fromEntries/`,
  `built-ins/Array/from/`, `language/expressions/super/`, `language/statements/class/`
  (+ `built-ins/Reflect/construct/`), `language/expressions/assignment/` +
  `compound-assignment/`, `language/statements/async-function/` + `built-ins/AsyncFunction/`.
  All six directories exist in the submodule (confirmed after `git submodule update --init
  --depth 1 test262`, which this fresh workspace needed).
- **`test262-extra/`**: run as part of the release-checked/`--bytecode` gate after all slices
  — it's the general regression net for engine-internal behavior that test262 itself doesn't
  pin, and nothing here is expected to change it.
- **No new `test262-extra/` or `tests/` files**: this refactor changes no observable behavior,
  so there is nothing spec-correct-but-untested to add. The one new test (slice 2) is an
  engine-internals test of GC root-stack balance, which belongs in `src/interpreter/tests.rs`
  next to the existing `with_gc_root_scope_truncates_on_every_exit`, not in `test262-extra/`
  (which is for spec-observable behavior) or `tests/` (host-compatibility/stress diagnostics).
- **`cargo test`**: runs the new unit test and exercises `debug_assert!`-gated root-balance
  checks for free (debug builds have `debug_assertions` on).
- **`cargo build --profile release-checked` + targeted/`test262-extra` runs**: the actual
  mechanism that would catch a botched frame conversion at release speed; CLAUDE.md already
  mandates this for GC root-stack discipline work.
- **`JSSE_GC_STRESS`**: per-slice commands above. This is the test that actually validates
  "no value went unrooted across a safepoint it used to be protected through" — a wrong
  conversion would show up as a stress-only failure (wrong-typed value, spurious `TypeError`,
  or a release-checked `debug_assert!` panic), not as a plain-build test262 failure.
- **Library harnesses**: `./scripts/run-library-tests.sh acorn` and
  `./scripts/run-library-tests.sh decimal.js` after slice 4 (cheap, already green, both use
  `Array.from`). Not run for the other slices — this is "polish," not warranting the full
  library sweep (`moment`, `zod`, `luxon` are each several minutes).

## 6. Regression risk

- **`test262-pass.txt` baseline**: must not move in either direction. Any changed outcome
  after a slice means that slice introduced a behavior difference (a missed exit path, wrong
  nesting order, or a value rooted too late) — stop and fix the slice, do not special-case the
  test or touch the baseline file.
- **Hot tree-walker paths**: `eval_assign` (every assignment) and the `super()` branch inside
  `eval_call` (every `super()` call, a narrower surface than `eval_call` as a whole) are the
  two sites gated on perf measurement in slices 5-6; everything else is cold enough (native
  builtin calls, class construction, async function kickoff) that closure overhead is noise.
- **GC rooting / safepoints**: every conversion must protect the exact same values across the
  exact same safepoints the manual frame did — reviewed per-site in §4. The main failure mode
  is subtly narrowing a scope (closing before a call that can still allocate) rather than
  widening it; closing later than necessary is always safe, closing earlier is not.
- **Two callers of `construct_from_evaluated`**: the tree-walker's `eval_new` (`eval.rs:6419`)
  and the bytecode VM's `Op::Construct` (`bytecode/vm.rs:562`) both call it directly, so slice
  2's regression net must run `--bytecode` test262 too, not just the default tree-walker path.
  The other five sites have no reference from `src/interpreter/bytecode/vm.rs` by name, but
  compiled functions can still call the two native builtins (`fromEntries`, `Array.from`) and
  can still bail to the tree-walker for `super()`/assignment/async-function bodies the
  compiler doesn't lower — so `--bytecode` test262 runs stay part of the net for all six, just
  with lower expected exposure for the tree-walker-only four.
- **Nested `with_gc_root_scope` (slice 4)**: this is the first production use of the
  combinator nested inside itself. The ADR says plain LIFO nesting composes fine, and §4's
  design keeps the inner call's lifetime strictly inside the outer closure's lexical body (no
  value crosses from inner to outer except through a plain return value), so this should hold,
  but it's the highest-complexity slice in this plan and gets the most scrutiny (targeted
  test262 + stress + library harnesses, not just targeted test262 + stress like the others).
- **`ObjectKind` exhaustive match / `property.rs` MOP**: untouched by this refactor — no new
  object kinds, no new property operations. Not a risk vector here.
- **Node-compat library harnesses**: not a hard gate for this issue (see §5), but acorn/
  decimal.js are cheap enough to run after slice 4 as extra signal beyond test262's own
  `Array.from` coverage.

## 7. Out of scope

The issue names exactly six sites; closing it means converting those six (minus whichever of
slices 5/6 a perf gate rejects, documented instead). Everything else the research for this
plan turned up stays on the raw `gc_root_frame`/`gc_unroot_frame` primitive, deliberately, as
material for separate follow-up issues:

- **`src/interpreter/builtins/promise.rs`** — 8 single-frame pairs (`setup_promise`'s two
  small `Promise.try` branches, `promise_all`, `promise_all_settled`, `promise_all_keyed`,
  `promise_all_settled_keyed`, `promise_race`, `promise_any`). Largest unconverted cluster;
  not examined for internal exit-path/`gc_unroot_value` mixing in this plan.
- **`src/interpreter/builtins/iterators.rs`** — ~10 single-frame helper methods on the
  Iterator Helpers prototype (`toArray`, `forEach`, `some`, and others) plus
  `iterate_to_vec`. Structurally simple (`loop { break Completion::X }`), likely easy
  follow-ups.
- **`src/interpreter/builtins/typedarray.rs`**'s `collect_iterable_or_arraylike` and
  **`src/interpreter/builtins/atomics.rs`**'s one pair — single straight-line calls, trivial
  conversions, just not named in the issue.
- **`src/interpreter/eval/literals.rs`**'s `create_regexp` and `copy_data_properties`.
- **`src/interpreter/eval.rs`**'s other manual pairs not named in the issue:
  `eval_logical_assign` (the `&&=`/`||=`/`??=` sibling of `eval_assign`'s converted branch),
  `destructure_object_assignment` (same easy-sibling shape as `eval_assign`), `eval_member_lhs_ref`,
  `eval_new`'s own outer frame (6406/6420 — distinct from `construct_from_evaluated`'s internal
  frame, which slice 2 does convert), `call_constructor_body`, the other two frame regions
  inside `eval_call` (direct-`eval()` fast path at 4789-4799, general call path at 4804-4891),
  and `async_function_resume`'s macro-expanded frame inside `park_dispose_at_await!`.
- **`src/interpreter/exec.rs`**'s `exec_for_of` (tiny, tight pair).
- **Deliberately excluded, not just deferred** (per the ADR, unchanged by this plan):
  `exec.rs`'s `bind_pattern` and `eval.rs`'s `destructure_array_assignment` — both tear down
  via `gc_unroot_value` (per-value identity) interleaved with frame-based rooting, which a
  bulk-truncating `with_gc_root_scope` cannot safely replace without the cross-branch-identity
  problem the ADR describes; `bytecode/vm.rs`'s `root_operand_stack` — a per-opcode safepoint
  mechanism inside the VM's dispatch loop, not a whole-native-function-body shape; `mod.rs`'s
  event-loop driver frames (`run`/`run_with_path`, `drain_microtasks`, `run_due_timers`,
  `drain_microtasks_blocking`) and `await_value`'s blocking loop — per-iteration driver
  frames paired with `gc_assert_root_depth`, not single-body natives; `call_function_inner_impl`'s
  native-call operand accounting (`eval.rs:5207-5223`) — already the deliberate per-value LIFO
  contract for native call operands, not a frame-truncate candidate.
- **No RAII `Drop`-guard**: the ADR already rejected this design (Design B); not revisited here.
- **`Array.fromAsync`**: already migrated off `gc_temp_roots` entirely onto `RootedSlots`
  (ADR, "Deliberately not migrated" section) — not relevant to this issue.
- **Formatting/unrelated cleanup** in any touched function beyond what the conversion itself
  requires.
