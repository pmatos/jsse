# Plan: issue #779 — async functions' `close_for_of_loop` iteration_env dispose still blocks

## 1. Problem restated

When an async function's `for (await using x of iterable)` loop is abruptly
exited (`throw`, `return`, or `break`/`continue` crossing the loop) and the
loop's per-iteration environment holds a resource whose
`[Symbol.asyncDispose]` itself awaits, `close_for_of_loop`
(`src/interpreter/eval.rs:9698`) disposes that `iteration_env` via
`dispose_resources` → `run_dispose_cursor_blocking`
(`src/interpreter/exec.rs:2786`, `src/interpreter/dispose.rs`), which calls
`self.await_value(&value)` **synchronously** on a `DisposeStep::Await` —
draining the job/microtask queue inline until the disposer's promise settles,
instead of suspending the async function and returning control to its
synchronous caller. This violates the same "driver suspends at its own
`Await`" invariant already fixed for the async-generator driver's equivalent
call path (issue #733, extended by #761; ADR-2026-09-22-2340 / -2326). Three
call sites in the async-function driver still hit this blocking path: the
`unwind_for_of!` macro (used by `route_return!`, `route_loop_control!`, and an
inline `Completion::Break` fast path) and `unwind_async_for_of_loops` (used by
the top-level throw-routing block). `PendingForOfUnwind` does not help here —
it only sequences a *second* pass through an unwind after some *other*,
already-resumable suspension settles; it never participates in this dispose's
own `Await`.

## 2. Spec basis

- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`
  (ForIn/OfBodyEvaluation)** — `spec/spec.html:22388`. Governs the per-iteration
  lexical environment (`iterationEnv`, built via `NewDeclarativeEnvironment`)
  and the requirement that an abrupt completion leaving the loop body closes
  via `IteratorClose`/`AsyncIteratorClose`. This is the clause every existing
  `async-function-for-of-*`/`async-generator-for-of-*` file in
  `test262-extra/` cites as `esid`, and the one the new test for this issue
  will cite too (matching `async-generator-for-of-throw-unwind-suspends.js`).
  (The spec source is ecmarkup auto-numbered emu-alg lists with no literal
  step numbers in `spec.html`'s raw text — cite the clause id, not invented
  step numbers.)
- **`sec-iteratorclose` / `sec-asynciteratorclose`** — `spec/spec.html:7162`,
  `:7220`. The synchronous IteratorClose tail (`close_for_of_iterator`,
  already correct and untouched by this fix).
- **`await` (Await)** — `spec/spec.html:51037`. Step: "Resume
  _callerContext_ passing ~empty~" after only registering `PerformPromiseThen`
  — an `Await` must hand control back to the caller, never block. This is the
  invariant `run_dispose_cursor_blocking`'s inline `await_value` call
  violates for these call paths; the fix makes each one use the same
  suspend/resume shape `async_fn_suspend_at_await`/`park_async_function_dispose`
  already implement for every other suspension point in this driver.
- **`sec-asyncblockstart` / `sec-async-functions-abstract-operations-async-function-start`**
  — `spec/spec.html:51007`, `:50991`. Governs how an async function body is
  driven and resumed via the job queue.
- **Resource disposal mechanics** (`using`/`await using`, `DisposeResources`,
  `SuppressedError`, `[Symbol.(async)dispose]`) are governed by the TC39
  `proposal-explicit-resource-management`, which is **not present in the
  currently pinned `spec/` (ecma262) submodule commit**
  (`270a490b3f8bf6f15bced16021ee0c3ff107f823`, 2026-01-21 — confirmed by
  grepping `spec/spec.html` for `dispose`/`DisposableStack`: zero matches).
  `test262/` (pinned separately, `aae8cf6e`) does carry ERM tests behind
  `features: [explicit-resource-management]`, and jsse's own
  `test262-extra/` files already cite the proposal's `DisposeResources`
  algorithm text inline in their `info:` blocks for exactly this reason (see
  `async-generator-for-of-throw-unwind-suspends.js:16-28`) — this plan follows
  that same convention. **This issue changes no dispose ordering,
  `SuppressedError` chaining, or any other ERM-observable semantics** — those
  are already correct (ADR-2026-09-22-2326/-2340) and must not change. It only
  changes *how* the engine suspends and resumes around an already-correct
  dispose's `Await`, governed by the `Await` AO above.

## 3. Files to touch

Engine:
- `src/interpreter/eval.rs`:
  - `unwind_for_of!` macro (8213-8272) generalized to take an explicit seed
    completion and `DisposeThen` tag — `($from, $seed, $then)` — mirroring its
    sibling `unwind_scopes_to!` (8284-8345), instead of always starting at
    `Completion::Empty`.
  - Its three call sites: `route_return!` (8386), `route_loop_control!`
    (8469), and the inline `Completion::Break` fast path (8888-8896, which
    gets converted to build a `LoopControlTarget` and call
    `route_loop_control!(target)`, mirroring the adjacent
    `Completion::Continue` handling at 8897-8918 exactly — not kept as a
    fourth bespoke park site).
  - `unwind_async_for_of_loops` (9765-9779) rewritten to be resumable; its
    single call site in the top-level `pending_exception` throw-routing block
    (`needs_for_of_unwind` branch, 8724-8747).
  - The `(DisposeThen, Completion)` resume-dispatch match (8557-8650) gains
    three new arm pairs (one per new `DisposeThen` variant, §4).
- `src/interpreter/eval/generator_runtime.rs` — `ForOfUnwindOutcome` (private,
  line 30) and `dispose_env_for_for_of_unwind` (private, line 6698) get
  `pub(super)` visibility so `eval.rs` (the parent module) can reuse them
  instead of duplicating the block-vs-park decision. No behavior change to
  the generator path.
- `src/interpreter/dispose.rs` — three new `DisposeThen` variants:
  `ForOfCrossReturn` (unit — the value rides in the cursor's own
  `Completion::Return(v)` result, mirroring `ScopeCrossReturn`),
  `ForOfCrossLoopControl(LoopControlTarget)` (payload, mirroring
  `ScopeCrossLoopControl`), `ForOfCrossThrow` (unit, mirroring
  `ScopeCrossThrow`).

Docs:
- `docs/adr/` — new ADR recording: the generalized `unwind_for_of!` seed/tag
  signature; reuse of `ForOfUnwindOutcome`/`dispose_env_for_for_of_unwind`
  across both drivers; the `ForOfCross*` naming; and the one subtlety that
  needed explicit handling (§6) — `unwind_for_of!`'s per-level "does a new
  throw stop at a closer handler than the original target" re-check
  (8232-8260) must re-run after *every* level's dispose, whether it finished
  synchronously or after a resume, not just once.
- `CONTEXT.md` — no new vocabulary.

Tests:
- `test262-extra/async-function-for-of-abrupt-unwind-suspends.js` (new).

## 4. TDD slices

**Core technique (same insight across every slice):** a dispose cursor
carries its own in-flight completion as the ADR-2026-09-22-2326 "cursor owns
the completion" rule — a `DisposeStep::Done` result either preserves that
seed unchanged or replaces it (only a *throw* seed gets suppression-chained;
`Empty`/`Return`/`Break` all get replaced outright on a disposer/IteratorClose
failure, confirmed by `close_for_of_iterator`'s own "a close failure replaces
normal, break, continue, or return" comment, 9750-9751). So as long as each
macro invocation seeds its unwind with the *same externally-visible*
completion it would otherwise reconstruct for its own post-unwind tail
(`route_return!`'s `ret_val`, `route_loop_control!`'s `Completion::Empty`
— already true today, `unwind_async_for_of_loops`' `Completion::Throw(exc)`),
a parked-then-resumed re-entry that extracts the value back out of the
resumed `Completion` and re-invokes the *same* macro from scratch is provably
equivalent to letting the original call continue uninterrupted — exactly the
existing `ScopeCrossReturn`/`ScopeCrossLoopControl`/`ScopeCrossThrow` idiom
(`eval.rs:8602-8622`), now extended to for-of. **Verified directly** (not just
inferred from the IteratorClose comment the earlier draft of this plan leaned
on): `DisposeCursor::new`/`finish` (`dispose.rs:39-59, 154-159`) store
whatever `completion` they're constructed with verbatim and return it
unchanged from `finish()` on the no-disposer-error path regardless of its
variant; `current_error` (the suppression chain) is seeded from the
completion only when it's already `Completion::Throw`, so `Empty` and
`Return(ret_val)` are provably interchangeable as a seed — confirming
`ForOfCrossReturn` can stay a unit variant with `ret_val` riding in the
cursor's own result, with no `DisposeThen: Copy` conflict. This also means
seeding with `Return(ret_val)` instead of `Empty` is strictly better, not
merely equivalent: `DisposeCursor::for_each_value` (`dispose.rs:138-142`) GC-roots
`Completion::Return(v)`'s value but not `Empty`'s (there is none), so the
in-flight return value is now kept alive for the whole duration of a parked
dispose, where today's `Empty` seed wouldn't root it via this path at all.
The one piece of
`unwind_for_of!`-specific behavior that doesn't fall out of this for free is
the per-level "a throw that emerged *during* this unwind may need to stop at
a handler closer than the original `$from` target" re-check (8232-8260); that
decision must be re-evaluated after each level's dispose settles, whether
synchronous or resumed, so it has to move into the per-level step rather than
staying as a one-shot loop body. `test262-extra/async-function-for-of-abrupt-completion-unwind.js`
(existing, sync-dispose-only) already exercises this logic exhaustively and
is the regression oracle for it; the new test (§5) adds the same shape with
an async disposer so it's exercised across an actual suspension too.

1. **Red:** add `test262-extra/async-function-for-of-abrupt-unwind-suspends.js`
   with the witness-chain technique from
   `async-generator-for-of-throw-unwind-suspends.js`, covering five
   independent shapes in one file, each its own assertion block:
   - (a) uncaught `throw` unwinding a single `await using`-bound loop whose
     disposer awaits (exercises `unwind_async_for_of_loops`/throw-routing).
   - (b) `return` doing the same (exercises `route_return!`'s
     `unwind_for_of!` call).
   - (c1) an unlabeled `break` in a body *with no intervening `await`*,
     crossing a single `await using`-bound loop whose disposer awaits —
     this is the only shape that reaches the inline `Completion::Break` fast
     path (8888-8896) rather than the `LoopControl` terminator.
   - (c2) a `break`/labeled-`continue` reached from a body that itself
     `await`s first, crossing two nested loops (so it is a genuine
     *cross*, not a same-loop `continue`) — this reaches the `LoopControl`
     terminator → `route_loop_control!` path. **Must not use an unlabeled
     `continue` targeting its own innermost loop** — that goes through
     `ForOfHead`'s per-iteration dispose (`DisposeThen::ForOfIteration`),
     which is already resumable today and would be silently green before
     this fix, defeating the red step.
   - (e) `return` inside an *inner* `await using`-bound loop whose async
     disposer **rejects**, with a `try`/`catch` sitting between the inner
     and outer loop (outer loop not `await using`-bound). The return
     becomes a throw mid-unwind; it must land at that `catch` — not
     propagate to the outer loop or past it — and the outer loop must still
     be open afterward. This is the shape that actually exercises
     `unwind_for_of!`'s per-level handler-boundary re-check (8232-8260)
     *across a suspension*; shape (a)'s throw-routing path does not reach
     that code at all (its `unwind_from` is fixed before the call, with no
     per-level re-check), so an earlier draft of this plan citing a
     throw-with-catch shape under throw-routing as the guard for this logic
     was wrong — (e) is the real guard, and it must run through
     `route_return!`'s `unwind_for_of!`, not through throw-routing.
   Run: `uv run python scripts/run-test262.py test262-extra/async-function-for-of-abrupt-unwind-suspends.js`
   — expect all five to fail today (wrong tick ordering / synchronous drain).
2. **Green — plumbing:** `generator_runtime.rs`'s `ForOfUnwindOutcome` and
   `dispose_env_for_for_of_unwind` become `pub(super)` (no logic change). Add
   the three `DisposeThen` variants to `dispose.rs`. `cargo build --release`
   must succeed with the new variants unused yet (or stubbed) — confirms no
   breakage before behavior changes land.
3. **Green — Slice A, throw-routing (shape a):** rewrite
   `unwind_async_for_of_loops` to mirror `unwind_generator_for_of_loops`'s
   shape (`generator_runtime.rs:6599-6651`): keep each loop on `for_of_stack`
   with `iteration_env` taken via `dispose_env_for_for_of_unwind` (reused,
   `can_park: true`) until its own dispose finishes, only then pop and call
   `close_for_of_iterator`; return `ForOfUnwindOutcome::{Done, Parked}`. Wire
   its single call site (8734-8746) to handle `Parked` with the same
   gc-root + `async_fn_suspend_at_await` + `park_async_function_dispose` +
   `return` sequence `unwind_scopes_to!` already uses (8300-8327), parking
   with `DisposeThen::ForOfCrossThrow`. Add the resume-dispatch arms:
   `(ForOfCrossThrow, Completion::Throw(e)) => { pending_exception = Some(e); }`,
   `(ForOfCrossThrow, _) => unreachable!(...)`. This alone makes shape (a)
   pass. Confirm via the test file.
4. **Green — Slice B, `route_return!` (shapes b and e):** generalize
   `unwind_for_of!` to `($from, $seed, $then)`; update its per-level body to
   use `dispose_env_for_for_of_unwind`/`close_for_of_iterator` the same way as
   Slice A (shared code, not reimplemented); preserve the handler-boundary
   early-stop re-check per level (8232-8260) — re-run it after *every*
   level's dispose result, park or no park. On park, bail out via the same
   suspend/park/return sequence, tagged `DisposeThen::ForOfCrossReturn`.
   Update `route_return!` (8386) to call
   `unwind_for_of!(unwind_from, Completion::Return(ret_val.clone()), DisposeThen::ForOfCrossReturn)`.
   Resume-dispatch arms:
   `(ForOfCrossReturn, Completion::Throw(e)) => { pending_exception = Some(e); }`,
   `(ForOfCrossReturn, Completion::Return(v)) => { route_return!(v); }`,
   `(ForOfCrossReturn, _) => unreachable!(...)`. Confirm shape (b) passes.
   For shape (e): the `(ForOfCrossReturn, Completion::Throw(e))` arm only sets
   `pending_exception`, not `pending_for_of_unwind` — unlike `unwind_for_of!`'s
   own synchronous Throw tail (8264-8270), which sets both before `continue`.
   This is intentional, not a gap: the park left the loop still on
   `for_of_stack` (iteration_env already taken, not yet popped), so when
   `pending_exception` reaches the top-level throw-routing block,
   `needs_for_of_unwind` (8687) is recomputed as true from that same
   `for_of_stack`, and `pending_for_of_unwind` gets (re)set correctly at
   8755-8763 if any loop remains open past the handler. Confirm this
   explicitly with shape (e) rather than assuming it — it is the one place
   this plan relies on throw-routing's own bookkeeping to finish a job
   `route_return!`'s macro started.
5. **Green — Slice C, `route_loop_control!` + `Completion::Break` (shapes c1,
   c2):** update `route_loop_control!` (8469) to call
   `unwind_for_of!(handler_boundary.min(for_of_stack.len()), Completion::Empty, DisposeThen::ForOfCrossLoopControl(target))`
   (seed unchanged from today — `Completion::Empty` is already what this call
   site uses). Resume-dispatch arms:
   `(ForOfCrossLoopControl(target), Completion::Throw(e)) => { pending_exception = Some(e); }`,
   `(ForOfCrossLoopControl(target), _) => { route_loop_control!(target); }`.
   Separately, convert the inline `Completion::Break` handler (8888-8896) to
   build a `LoopControlTarget { target_state: after_state, try_depth:
   for_of_stack[pos].try_depth, for_of_depth: pos, scope_depth:
   scope_stack.len() }` and call `route_loop_control!(target)`, deleting its
   own direct `unwind_for_of!(pos)` call — mirroring the adjacent
   `Completion::Continue` arm exactly (8901-8916), which already does this.
   **Before wiring this up, diff what `route_loop_control!` does that the
   inline arm doesn't**: `routed_to`/finally dispatch (8432-8440),
   `pending_for_of_unwind = None` (8430), and `try_stack.truncate(target.try_depth)`
   in the no-`routed_to` tail (8489) — the inline arm today does none of
   these. For each, confirm it's either unreachable from the inline-break
   state today (e.g. no open `finally` can exist there) or is genuinely a
   spec-required behavior the inline path was silently skipping. If the
   latter — e.g. if today's inline `break` can cross an un-entered `finally`
   without running it — the conversion is a **behavior fix**, not a neutral
   refactor, and needs its own dedicated test plus a PR note calling it out
   explicitly, not just "existing tests still pass." Run the **existing**
   `generator-loop-control-closes-for-of-iterators.js` and
   `async-function-for-of-abrupt-completion-unwind.js` first, before adding
   any new resumability, to catch any such behavior change in isolation from
   the new suspend/resume logic; then confirm shapes (c1)/(c2) pass with
   resumability added.
6. **Confirm green + no regressions:** full Slice-1 test file green. Run
   `cargo build --release`, full targeted test262 surface (§5),
   `cargo test --release`.

## 5. Test surface

- **Targeted test262 run** (regression check — ERM's `for-of` head tests
  don't assert tick ordering, so no new passes expected, only "still green"):
  `uv run python scripts/run-test262.py test262/test/language/statements/for-of/`
  and
  `uv run python scripts/run-test262.py test262/test/language/statements/async-function/`.
- **Staging ERM suite** (run explicitly per CLAUDE.md, not part of the
  default suite): `uv run python scripts/run-test262.py test262/test/staging/explicit-resource-management/`.
- **New test262-extra test** (the actual regression coverage for this bug —
  test262 itself doesn't assert suspension/tick-ordering):
  `uv run python scripts/run-test262.py test262-extra/async-function-for-of-abrupt-unwind-suspends.js`.
- **Existing regression guards, must stay green unchanged:**
  `test262-extra/async-function-for-of-abrupt-completion-unwind.js` (the
  handler-boundary/ordering oracle, sync-dispose only),
  `test262-extra/generator-loop-control-closes-for-of-iterators.js`,
  `test262-extra/async-function-for-of-return-closes-iterators.js`,
  `test262-extra/async-function-nested-for-of-await-per-iteration-environments.js`,
  and the async-generator suspend tests
  (`async-generator-for-of-throw-unwind-suspends.js`,
  `async-generator-for-of-unwind-resumes-all-actions.js` — confirm the
  `pub(super)` visibility change didn't regress the generator side).
- **Full suite:** `uv run python scripts/run-test262.py` (baseline-diff
  against `origin/main:test262-pass.txt`, no `--update-baseline`).
- **Rust tests:** `cargo test --release` (includes
  `tests/test262_smoke_oracle.rs`'s sampled cross-check).

## 6. Regression risk

- **Shared machinery leaned on:** `async_fn_suspend_at_await` /
  `park_async_function_dispose` (already generic, reused as-is); the
  `(DisposeThen, Completion)` resume-dispatch match in `async_function_resume`
  (new arms added, existing arms untouched); GC rooting around a parked
  `DisposeCursor` (copied from the existing `unwind_scopes_to!` pattern).
- **Highest-risk piece:** the handler-boundary early-stop re-check
  (`unwind_for_of!`, 8232-8260) moving from "evaluated once per macro
  expansion" to "evaluated per level, synchronous or resumed" (Slice B). A
  mistake here would misroute an exception that arises from a disposal
  failure to the wrong `catch`/`finally` — behavior, not a crash, so it needs
  the dedicated shape-(e) test (§4.1), not shape (a) or (d) from an earlier
  draft of this plan (throw-routing's `unwind_async_for_of_loops` has a fixed
  `unwind_from` and never reaches this per-level re-check at all — only
  `route_return!`'s/`route_loop_control!`'s `unwind_for_of!` call does), plus
  the existing `async-function-for-of-abrupt-completion-unwind.js` oracle, not
  just a build-succeeds check.
- **Second touch to already-shipped code:** `ForOfUnwindOutcome`/
  `dispose_env_for_for_of_unwind` visibility change (`pub(super)`, purely
  additive) — run the full generator-suspend test262-extra files (§5)
  explicitly.
- **`Completion::Break` → `route_loop_control!` conversion:** a refactor of
  existing (currently-correct, currently-blocking) behavior, not just new
  resumability — verified against the existing break/continue-closes-iterator
  tests *before* resumability is added on top (Slice C's ordering, §4.5). If
  the implementer finds the inline arm was skipping a `finally`/leaving
  `pending_for_of_unwind` stale/under-truncating `try_stack` relative to what
  `route_loop_control!` does (§4.5's diff), that's a behavior fix riding
  along with this PR and must be called out in the PR description and covered
  by its own test, not folded silently into "resumability added."
- **What could move `test262-pass.txt`:** nothing in test262 proper currently
  exercises this ordering (confirmed in §5), so no baseline movement is
  expected; a regression would show up as new *failures*. The baseline itself
  is not rewritten from this branch regardless, per CLAUDE.md.
- **Tree-walker hot path / bytecode fast path / `property.rs` MOP /
  `ObjectKind` matches:** not touched — this is async-function-driver-only
  logic.
- **Node-compat library harnesses:** unaffected — no pinned library fixture
  exercises `await using` inside `for-of` in an async function; standard full
  run is sufficient, no dedicated harness action needed.

## 7. Out of scope

- **`ArrayPatternIterOp::Finish`** (`eval.rs:9570-9594`, array-destructuring
  rest pattern, e.g. `[a, ...rest] = someIterable`) — **not a blocking gap at
  all, on either driver.** Verified directly: `iteration_env` is only ever
  set to `Some(...)` by `ForOfHead`-equivalent lexical-binding states
  (`eval.rs:9403`, `generator_runtime.rs:1809`, `generator_runtime.rs:5215`);
  `ArrayPatternIterOp::Init` always constructs its `ForOfLoopState` with
  `iteration_env: None` (`eval.rs:9483`) and nothing ever mutates it to
  `Some` afterward for an array-pattern-sourced loop state — array
  destructuring assignment has no per-iteration lexical environment concept.
  So `close_for_of_loop`'s call there is always pure, synchronous
  `IteratorClose` via the `None` branch; it was mis-flagged as a "known
  blocking gap" during research and does not need a follow-up.
- **`close_for_of_loop`'s own signature/behavior** — unchanged; remains the
  blocking whole-env entry point for the sync-generator driver, where
  blocking is correct by design (ADR-2026-09-22-2340's `can_park: false`
  choice — sync generators cannot suspend mid-dispose at all) and for
  `ArrayPatternIterOp::Finish` (never actually blocks, per above, so no
  behavior difference either way).
- **Any ERM ordering/`SuppressedError`-chaining semantics** — already correct
  per ADR-2026-09-22-2326/-2340; this PR must not change observable ordering,
  only *how* control returns to the caller at the dispose's own `Await`.
- **Rewriting `test262-pass.txt`** — not performed from this branch per
  CLAUDE.md; any baseline movement (none expected, §6) is a `main`-branch
  operation.
