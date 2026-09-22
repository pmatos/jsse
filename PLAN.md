# Plan: issue #733 — async generators: for-of unwind, inline replay and delegated yield* exits still skip suspending disposal

## 0. Scope decision (read this first)

Issue #733 bundles four independent defects at very different depths. Investigation (below)
found that **item 4, as literally written, is already fixed** by #737 (merged just before this
branch was cut), and that **item 1 is the one structurally-clean, well-precedented piece of work**
left. Items 2 and 3 are real but require either an upstream prerequisite (item 2) or a large,
higher-risk restructuring of the delegated-`yield*` completion paths (item 3). Cramming all of
that into one PR violates "many small changes beat one large change." This plan implements
**item 1 (for-of unwind resumability) plus a regression test for item 4**, and documents items 2
and 3 as follow-up work with the reasoning needed to scope them later.

The implementation stage must open the PR as **"part of #733"**, not "closes #733" — file a
follow-up issue enumerating items 2 and 3 (mirroring how #733 itself was split from #716) before
or when opening the PR, and reference it in the PR body. Post a `gh issue comment 733` recording
this scope split (with the item-4-already-fixed finding) as soon as the split is final — that is
a judgement call worth surfacing per the operating contract, and it costs nothing to record now.

## 1. Problem restated

The async-generator driver (`src/interpreter/eval/generator_runtime.rs`) suspends correctly at
ordinary `yield`/`await` points and at most `DisposeResources` sites (ADR-2026-09-21-2015,
ADR-2026-09-21-2246, ADR-2026-09-21-2300, ADR-2026-09-22-2326), but one class of disposal still
drains the microtask queue synchronously inside the call that triggers it: closing a transformed
`for-of` loop that a `throw`, `break`/`continue`, `return`, or normal state-to-state fallthrough is
unwinding through. When that loop's per-iteration `await using` resource (or a scope nested inside
it) has an async disposer, `unwind_generator_for_of_loops` and its helpers
(`dispose_scopes_inside_for_of`, and `close_for_of_loop`'s iteration-environment step) run that
disposer's `Await` to completion inline instead of suspending the driver, exactly the drain-inside-
the-call bug already fixed at every *other* disposal site in this driver. This is observable: a
disposer with an `await` runs, and any code queued before the triggering `.next()`/`.throw()`/
`.return()` call executes, before the call returns control to its caller.

## 2. Investigation findings that reshape the scope

### 2.1 Item 4 (for-of-with-`await using` nested in `try`/`if`) is already fixed

Commit 96a83253 (#737, "recurse into containers when detecting for-await suspension", already on
this branch) fixed `stmt_contains_for_of_head` in `src/interpreter/generator_transform.rs` so its
`Statement::ForOf` arm recurses into `f.body` (previously `head(f)` alone, so a `for-of` nested
inside *another* loop's body wasn't found). Because `stmt_has_suspension`'s for-of-head check
(`(is_async && f.disposes_at_head()) || …`) is unconditional on `is_async` (not gated on
`detect_for_await`, which is function-only) and `stmt_contains_for_of_head` already recursed into
`If`/`Try`/`Block`/`Labeled`/`While`/`Switch`/`With` before #737, a `for (await using x of y)`
nested in `try`/`if` was already detected pre-#737; #737 closed the one remaining gap (loop-nested-
in-loop). I verified empirically rather than trusting the reasoning alone: built this branch in
release mode and ran a witness-chain probe (four `Promise.resolve().then()` jobs queued *before*
calling `.next()`, so blocking disposal would run them early) against both

```js
async function* g() {
  if (true) {
    for (await using x of [makeAsyncDisposable("a")]) { /* ... */ }
  }
}
```

and the same shape inside `try { … } finally { … }`, and the equivalent `async function`. In every
case jsse's log order matched Node's exactly (`after-next` before `w1..w4`, disposal only once the
pre-queued jobs drain) — no inline blocking. **No code change for item 4; add a locked-in
regression test only** (slice 6).

### 2.2 A related-but-distinct gap exists and is in scope as a bonus, clearly labelled

The same probe technique against a **plain `await using` declaration with no `for-of`**, nested in
`if` with no other `await`/`yield`:

```js
async function* g() {
  if (true) {
    await using r = makeAsyncDisposable("a");
  }
}
```

reproduces the inline-blocking bug on this branch (jsse: `after-next` logged *last*, after the
disposer and all four pre-queued jobs; Node: `after-next` logged immediately). Root cause:
`stmt_has_suspension` (`src/interpreter/generator_transform.rs:649`) only consults
`has_suspendable_await_using_block` (`src/interpreter/generator_analysis.rs:1141`, the container-
recursing scanner for a bare `await using` block, as opposed to a for-of head) when
`detect_for_await` is true — and `detect_for_await` is `true` only for `transform_async_function`
(`generator_transform.rs:3086`), never for `transform_async_generator`
(`generator_transform.rs:476-481`, `detect_for_await` hardcoded `false`). The top-level bail-to-
`create_simple_machine` check (`generator_transform.rs:517`) has the identical gate. This is
ADR-2026-09-21-2015's own "known boundary" ("An `await using` in [a container] with no
`await`/`yield` inside is still tree-walked and disposes inline"), never superseded by
ADR-2026-09-22-2326. It is **not** issue #733's item 4 (which is about the for-of-head case, and is
fixed) — it is called out here because the investigation surfaced it and the fix is small,
mechanical, and isolated. Included as slice 7, clearly flagged as outside the four listed items so
the implementation stage (or a reviewer) can drop it without touching the rest of the PR.

### 2.3 Item 1's precedent and design

`unwind_generator_for_of_loops` (`generator_runtime.rs:6045`) is **shared with the sync generator
driver** (`generator_next_state_machine_impl`'s `route_exception!`/`route_loop_control_result!`
macros at `generator_runtime.rs:839-884`, both calling the same `route_generator_exception`/
`route_generator_loop_control`). A sync generator's dispose stacks can only ever hold `using` (never
`await using`, a syntax error outside async contexts), so `DisposeCursor::step` provably never
returns `DisposeStep::Await` for a sync caller — the new resumable path must make that an explicit
invariant (`unreachable!()`), not silently assume it.

The five callers named in the issue break into two groups by what they can use as a resume carrier:

- **Have a natural carrier already** (the completion the `DisposeCursor` itself carries, exactly
  as the frame-leave loop already does at `generator_runtime.rs:3684-3730`): `route_generator_
  exception`, the `pending_return` block (`generator_runtime.rs:3552-3626`), and the bare
  `return;` `StateTerminator::Return` arm (`generator_runtime.rs:4346`). **Do not** save the
  triggering value into `pending_exception`/`pending_return` *and* seed the cursor with it — that
  double-books the completion. Follow the frame-leave loop's own precedent exactly: save
  `pending_exception: None, pending_return: None` in the parked `IteratorState`, seed the
  `DisposeCursor` with the in-flight `Completion::Throw`/`Completion::Return`, and let
  `async_gen_reenter` (`generator_runtime.rs:5366`) restore *whatever the cursor finishes with*
  (a disposer's throw legitimately replaces an in-flight return — that is already
  `async_gen_reenter`'s documented behavior, not new). `Reenter` restarts
  `async_generator_next_state_machine_impl` from the top, `check_abrupt_on_resume` fires with the
  restored completion, and the *same* routing function runs again. This is safe to re-run because
  the loop stays on `for_of_stack` with `iteration_env` already taken (`None`) and any scope
  frames nested inside it already popped from `generator_scope_stacks`: a second pass finds
  nothing left to await and completes synchronously, then proceeds exactly as a first pass that
  never parked would have. The saved `try_stack` must be the one *after*
  `truncate(loop_state.try_depth)` for the loop(s) already closed before the park — not the
  pre-unwind stack — or a resumed re-route can pick a handler that was lexically inside the loop
  being closed.

  **The `return expr;` arm (`generator_runtime.rs:4207`) does not belong in this group as a
  drop-in reuse.** Today, when no enclosing `finally` applies, this arm unwinds for-of loops with
  the *raw* (not-yet-`Await`ed) expression value, and only *after* that unwind performs
  `Await(exprValue)` itself via a `wrapper`/`chain_promise`/`GeneratorDisposeState::ReturnOperand`
  step (`generator_runtime.rs:~4335-4345`) — a mechanism ADR-2026-09-22-2326 documents as
  deliberately distinct from the generic `pending_return` block, which instead documents `pending_
  return` as *always already-awaited*. Resuming a parked unwind from this call site through
  `Reenter`→`check_abrupt_on_resume`'s generic `pending_return` handling would skip that
  `Await(exprValue)` step and settle the request with the un-awaited value — a real correctness
  bug, not a style issue. The implementation stage must pick one of:
  (a) reorder to `Await(exprValue)` *before* the for-of unwind, matching how the sibling
  `finally`-present branch already awaits before its unwind (per the ADR: "`return expr;` through
  a `try/finally` reaches the same function, so its single `Await(exprValue)` now precedes the
  `finally`") — the more uniform fix, but it moves a tick relative to for-of disposal ticks and
  must be checked against existing tick-count pins in `test262-extra/` and the `for-await-of` tick
  tests before landing; or
  (b) keep the current ordering and carry an explicit "operand not yet awaited" marker across the
  park so resume lands back in the wrapper/chain_promise step, not the generic `pending_return`
  path.
  This plan does not prescribe which; slice 3 below must resolve it with a red test that would
  catch the bug in option (a) done wrong (an un-awaited thenable/rejecting promise as the return
  operand, unwind through a for-of loop that itself parks) before green.

- **No natural carrier** (`Completion::Empty`-seeded): `route_generator_loop_control` (called for
  `StateTerminator::LoopControl`, `generator_runtime.rs:4439-4447`) and `align_generator_for_of_stack`
  (called for every `StateTerminator::Goto`, `generator_runtime.rs:4450-4470` — the hottest of the
  five call sites). Async functions have an analogous carrier already
  (`PendingForOfUnwind`/`AsyncFunctionState.pending_for_of_unwind`, `src/interpreter/types.rs:386`,
  `393-410`) but it lives on a dedicated per-call struct that async generators don't have (their
  resumable state is split between `IteratorState::StateMachineAsyncGenerator`'s fields — touched
  by ~50 struct-literal sites in this file — and three side tables keyed by `generator_id`:
  `generator_for_of_stacks`, `generator_scope_stacks`, `generator_pending_dispose`). Adding a field
  to the `IteratorState` variant to carry this is exactly the editing burden ADR-2026-09-21-2015
  rejected for `generator_pending_dispose` ("A new field would have meant editing every literal
  construction"). Follow that precedent: add a **new side table**,
  `Interpreter::generator_pending_for_of_retry: FxHashMap<u64, ForOfRetryTarget>`
  (`src/interpreter/mod.rs`, beside `generator_pending_dispose` at line 278/629), where
  `ForOfRetryTarget` is a small enum (`LoopControl(LoopControlTarget) | Align(usize)`, both fields
  already `Copy`/GC-value-free — `LoopControlTarget` holds only `usize`s, so **no GC rooting entry
  is needed**, only cleanup on generator retirement, mirroring `gc.rs:755`'s
  `generator_pending_dispose.remove(&id)`). On park, insert the target being retried; a new
  resume-time check — symmetric with, and placed immediately alongside,
  `check_abrupt_on_resume`'s existing block at the top of the state loop — removes and re-invokes
  it *before* any state-body execution, then falls through to normal dispatch. `Reenter` still
  drives the restart; the new table only supplies what to redo once restarted.

  **A disposer that throws while a loop-control/align unwind is parked must not leave a stale
  retry entry.** The `Empty`-seeded cursor can still finish `Throw` (a disposer's own error) or
  `Exit`. `async_gen_reenter` already sets `pending_exception`/handles `Exit` for any completion
  it is given (`generator_runtime.rs:5376-5389`), so on resume both `generator_pending_for_of_
  retry` (still holding the loop-control/align target) and `pending_exception` could be set at
  once. The new resume-time check must: remove the retry entry unconditionally on resume (never
  leave it for a later, unrelated request to pick up), and only re-invoke the saved loop-control/
  align target when the restored completion carries neither an exception nor an exit — if
  `pending_exception` is set instead, defer to `check_abrupt_on_resume`'s existing throw handling,
  which calls `route_generator_exception` and unwinds whatever loops are still open below the one
  that just finished closing. This mirrors today's synchronous
  `Err(Completion::Throw(error)) => { let error = route_exception!(error); … }` arm in
  `route_loop_control_result!` (`generator_runtime.rs:3504-3511`, sync copy `:871-876`) — check
  for an exception/exit *before* consulting the retry table, not after.

### 2.4 `close_for_of_loop` is shared with async functions too, and is out of scope beyond generators

`close_for_of_loop` (`src/interpreter/eval.rs:9645`) is called by the async-generator driver, the
sync-generator driver, *and* async functions' own `unwind_for_of!` macro
(`eval.rs:8315-8374`, via `unwind_async_for_of_loops`, `eval.rs:9693`). Its `iteration_env` dispose
(`eval.rs:9652-9655`) calls `dispose_resources` (`exec.rs:2752`), which is unconditionally blocking
(`run_dispose_cursor_blocking`) — for every caller, today. Async functions' `PendingForOfUnwind`
solves a *different* problem (sequencing an abrupt completion through intervening handlers across a
suspension that already happened elsewhere, e.g. at a nested `await using` block scope via
`unwind_scopes_to!`/`async_fn_suspend_at_await`) — it does not make `close_for_of_loop`'s own
`iteration_env` Await resumable. **This PR does not fix that for async functions**; the issue's
scope is `generator_runtime.rs`, and touching `eval.rs`'s async-function behavior is unrelated
refactor creep. To keep async generators' fix from reaching into that shared function's behavior
for other callers, `close_for_of_loop` is split by pure extraction: pull the iterator-close half
(the part after `iteration_env` is handled) into a new `close_for_of_iterator` taking a loop_state
whose `iteration_env` is already `None`. `close_for_of_loop` becomes `dispose iteration_env` (as
today, blocking) `+ close_for_of_iterator` — a behavior-preserving refactor for its existing
callers (sync generators, async functions), verified by the existing test suite, not a new test.
The async-generator path calls a new resumable iteration_env-dispose helper (generator_runtime.rs-
local, using `DisposeCursor`/`park_async_gen_disposal` like the frame-leave loop already does at
`generator_runtime.rs:3696-3730`) followed by the shared `close_for_of_iterator` (IteratorClose is
synchronous per spec for a non-`for-await` loop and unaffected either way).

## 3. Spec basis

- **`IteratorClose`** (`sec-iteratorclose`) and **`ForIn/OfBodyEvaluation`**
  (`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`) govern
  when and how a `for-of` loop's iterator is closed on an abrupt completion — the behavior being
  made non-blocking, not changed.
- **`DisposeResources`** (`sec-disposeresources`) and the per-iteration binding disposal it
  performs for `await using` ForDeclarations — cited as in the existing `dispose.rs` comments —
  come from the **Explicit Resource Management proposal**, not yet merged into the pinned
  `spec/` (tc39/ecma262) snapshot at commit `270a490b` (verified: no `DisposeResources`,
  `disposecapability`, or "resource management" text in `spec/spec.html`). This mirrors the
  engine's existing, already-shipped implementation of `using`/`await using` (ADR-2026-09-21-2015
  onward), which cites the same proposal by clause id in code comments (`dispose.rs:24`). No new
  syntax or semantics are introduced here — only *when* an already-correct disposal's `Await`
  suspends the driver — so this is squarely "grounded in a spec clause," just one sourced from the
  proposal repo the engine already treats as authoritative for this feature.
- **`sec-runtime-semantics-forinofheadevaluation`** governs `await using` ForDeclarations
  specifically (the `disposes_at_head`/`awaits_at_head` predicates in `src/ast.rs:758-773`) —
  unaffected by this plan; item 4's investigation (§2.1) touches no code.
- No JavaScript-observable syntax or semantics change: every scenario this plan fixes already
  produces the spec-correct *result* (the ADRs above established the value/completion the engine
  returns); the fix is strictly about *when* control returns to the caller relative to queued
  microtasks (the driver no longer drains jobs that spec ordering says should run later).

## 4. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the resumable `unwind_generator_for_of_loops` /
  `dispose_scopes_inside_for_of`, the new iteration-env-dispose helper, the five caller sites
  (`route_generator_exception`, `route_generator_loop_control`, the `pending_return` block at
  ~3552-3626, the two `StateTerminator::Return` arms at ~4207/~4346, `align_generator_for_of_stack`),
  and the `route_loop_control_result!` macro's trailing blocking `dispose_resources` call
  (~3506, converted to the existing `dispose_or_park!` pattern since the macro is touched anyway).
- `src/interpreter/eval.rs` — extract `close_for_of_iterator` out of `close_for_of_loop` (~9645),
  pure refactor, no behavior change for its existing (sync-generator, async-function) callers.
- `src/interpreter/mod.rs` — new field `generator_pending_for_of_retry: FxHashMap<u64,
  ForOfRetryTarget>` (declaration + `Interpreter::new` initialization, beside
  `generator_pending_dispose` at lines 278/629).
- `src/interpreter/gc.rs` — cleanup of `generator_pending_for_of_retry` on generator retirement
  (beside `gc.rs:755`'s `generator_pending_dispose.remove(&id)`); no GC-root registration needed
  (the enum holds no `JsValue`). Guard the removal with an `is_empty()` check the way
  `sync_generator_for_of_stack` does (`generator_runtime.rs:6110-6125`) if retirement runs on
  every generator regardless of whether this table was ever populated for it, to avoid hashing an
  id into an empty map on the hot common case (no `for-of` at all).
- `src/interpreter/types.rs` or `src/interpreter/dispose.rs` — new `ForOfRetryTarget` enum
  (`LoopControl(LoopControlTarget) | Align(usize)`), placed near `PendingForOfUnwind`
  (`types.rs:386`) since it plays the same role for the generator driver.
- `src/interpreter/generator_analysis.rs` and `src/interpreter/generator_transform.rs` — slice 7
  only (the bonus block-form fix in §2.2): broaden the `detect_for_await` gate on
  `has_suspendable_await_using_block` to `is_async` at `generator_transform.rs:517` and `:663`.
- `docs/adr/` — a new ADR (e.g. `docs/adr/<date>-async-generator-for-of-unwind-suspends.md`)
  recording the `ForOfRetryTarget` side-table decision and the `close_for_of_loop` extraction,
  following the house style of the existing async-generator ADR series; supersede/update the
  "Known boundaries" section of ADR-2026-09-22-2326 (item 1) and note item 4's resolution there
  too since that ADR is what #733 quotes.
- `CONTEXT.md` — if it names the disposal side-table pattern already (check during
  implementation); add `ForOfRetryTarget`/the retry table if the domain doc tracks this level of
  vocabulary, consistent with how `generator_pending_dispose` was documented.
- `test262-extra/` — new regression files (§6).

## 5. TDD slices

Each slice is a vertical, independently-buildable-and-testable step. Slices 1-5 implement item 1;
slice 6 locks in item 4 (no production change); slice 7 is the clearly-labelled bonus fix.

Before slice 2 (the first slice with a witness-chain test), snapshot the current release binary
to `$TMPDIR` (per `CLAUDE.md`'s "never rebuild while a run is in flight" guidance — here used so
the red state of each new test can be re-confirmed against the pre-change binary without a
rebuild if a later slice's changes are suspected of masking an earlier one going green for the
wrong reason). Confirm every new `test262-extra` file is actually red against that snapshot before
implementing the fix that turns it green — slice 4's normal-exhaustion case in particular is not
guaranteed to be red (see below), and the plan's confidence that slices 2/3/5 are red rests on the
§2.1 probe methodology, not on having built each exact reduced case yet.

1. **Extract `close_for_of_iterator` from `close_for_of_loop`** (`eval.rs`). Red: none needed (pure
   refactor) — green is "the full test262 suite and `cargo test --release` are unchanged," run
   before and after as the check. This is preparatory for slice 2 and touches no behavior.
2. **Resumable iteration-env disposal + `unwind_generator_for_of_loops` returns
   `Done(Completion) | Parked`, with the loop retained on `for_of_stack` (iteration_env taken)
   across a park.** Test: a new `test262-extra/async-generator-for-of-throw-unwind-suspends.js`
   using the witness-chain technique from §2.1 (pre-queue several jobs, assert they run before the
   disposer when a `throw` inside `for (await using x of iter) { throw e; }` unwinds the loop).
   Red on current `main`/this branch; green once `route_generator_exception` (the first of the five
   callers, chosen because it already has a natural resume carrier) is wired through the new
   primitive. Production code: the new `ForOfUnwindOutcome`-returning unwind plus
   `route_generator_exception`'s caller-side handling (save state via the same pattern as
   `generator_runtime.rs:3696-3730`, `park_async_gen_disposal` with `GeneratorDisposeThen::Reenter`).
3. **Wire the `pending_return` block and the bare-`return;` terminator arm through the same
   primitive; resolve the `return expr;` arm's Await-ordering question (§2.3) with its own red
   test first.** Tests: `async-generator-for-of-return-call-unwind-suspends.js` (`.return()`
   invoked while suspended inside the loop), `async-generator-for-of-return-statement-unwind-
   suspends.js` (bare `return;` executed inside the loop body), and
   `async-generator-for-of-return-expr-unwind-suspends.js` (`return somePromise;` inside the loop,
   asserting *both* that the driver doesn't block on the for-of unwind *and* that the resolved
   value is the promise's resolution, not the promise itself — the second assertion is what would
   fail if resuming skipped `Await(exprValue)`). The first two share the "natural carrier" resume
   path (§2.3) and are mechanical repetition of slice 2's pattern; the third needs whichever of
   (a)/(b) from §2.3 the implementation stage picks, and its test must be written and shown red
   against the *other* option too (not just against today's code) if there's any doubt which
   option is correct.
4. **Add `ForOfRetryTarget` + `generator_pending_for_of_retry`, wire
   `route_generator_loop_control` and `align_generator_for_of_stack`.** Tests:
   `async-generator-for-of-break-unwind-suspends.js` (`break`/labelled `break` crossing the loop)
   and `async-generator-for-of-continue-unwind-suspends.js` (`continue` on an outer loop crossing
   this one) exercise `route_generator_loop_control` and should be red on this branch. A test for
   `align_generator_for_of_stack`'s `Goto` path (normal loop exhaustion) is *not guaranteed to be
   red*: by the time a loop exhausts, its last iteration's `ForOfHead` disposal
   (ADR-2026-09-22-2326 decision 4, already resumable) has likely already cleared
   `iteration_env`, leaving `align_generator_for_of_stack` nothing to await. Before writing that
   test, check whether any shape (e.g. a scope nested inside the loop body itself, disposed via
   `dispose_scopes_inside_for_of` rather than the loop's own `iteration_env`) reaches `align_
   generator_for_of_stack` with something still pending; if none does, say so in the PR instead of
   asserting a red test that was actually already green, and cover the plumbing with a
   `cargo test --release` unit test on the driver function directly instead. This is the slice
   that needs the new resume-time check (symmetric with `check_abrupt_on_resume`) at the top of
   `async_generator_next_state_machine_impl`.
5. **`route_loop_control_result!`'s trailing function-level dispose goes through
   `dispose_or_park!`.** Test: `async-generator-for-of-break-disposer-throws-suspends.js` — a
   disposer that itself throws during a `break`-triggered close that escapes every enclosing
   `catch`/`finally`, asserting both that the driver doesn't block and that the correct error
   (disposer's, chaining per `DisposeResources` semantics if applicable) rejects the request.
6. **Item 4 regression, no production change.** Add
   `test262-extra/async-generator-for-of-await-using-nested-in-container-suspends.js` (the `if`/
   `try`-nested for-of-with-await-using shapes from §2.1, for both async generator and async
   function) asserting the already-correct suspending order, so a future regression is caught.
7. **Bonus (§2.2), clearly separated commit/section in the PR:** broaden the `detect_for_await`
   gate to `is_async` in `stmt_has_suspension` (`generator_transform.rs:663`) and the top-level
   bail check (`generator_transform.rs:517`). Test:
   `test262-extra/async-generator-await-using-block-in-container-suspends.js`, the reproduction
   from §2.2 (bare `await using` in `if`, no `for-of`), red before, green after. Regression guard:
   rerun the equivalent async-*function* shape in the same test to confirm broadening the gate to
   `is_async` (which is `true` for both async functions and async generators, so this is a no-op
   for functions — `detect_for_await` was already `true` there) doesn't change function behavior.
   The claim that only these two gates need to change is inferred from reading
   `generator_transform.rs`, not from having built and probed it (unlike §2.1/§2.2, which were
   empirically confirmed) — the implementation stage must still show red-then-green on the actual
   built binary, and should check whether `scan_await_using`'s `Blocked` (lexical-flattening)
   outcomes behave identically for generators before trusting the two-line fix; if they don't,
   drop this slice into the follow-up issue rather than force it.

## 6. Test surface

- **Targeted test262 run** (must stay green, no regressions): `test262/test/language/statements/
  for-await-of/` (616 async-generator-shaped files plus `head-await-using-init.js`/
  `head-using-init.js`), `test262/test/built-ins/AsyncGeneratorPrototype/{next,return,throw}/`,
  `test262/test/language/statements/for-of/`, `test262/test/staging/explicit-resource-management/`.
  Run via `uv run python scripts/run-test262.py <dir>` per directory, plus the full untargeted
  suite before considering the slice done (`uv run python scripts/run-test262.py`, baseline from
  `origin/main:test262-pass.txt`, no `--update-baseline`).
- **Not covered by test262**: none of test262's `for-await-of`/`AsyncGeneratorPrototype` tests
  assert *microtask ordering* around disposal (they assert final values/completions, which are
  already correct today) — the entire defect this plan fixes is an ordering/blocking bug, invisible
  to a test that only checks the eventual result. Every new assertion in §5 therefore belongs in
  `test262-extra/`, following the witness-chain pattern already established by
  `test262-extra/async-generator-inline-yield-*.js` (from #729) and named after the spec clause
  under test (`IteratorClose`/`DisposeResources`/`ForIn/OfBodyEvaluation` ordering).
- `cargo test --release` and `./scripts/lint.sh` after every slice.
- `./scripts/run-mutants.sh --file src/interpreter/eval/generator_runtime.rs` optionally, given the
  density of new branches (`Parked` vs `Done` per call site) this introduces — not required by
  CLAUDE.md but a natural fit for the new match arms if time allows; not blocking for merge.

## 7. Regression risk

- **Sync generator driver**: shares `unwind_generator_for_of_loops`/`route_generator_exception`/
  `route_generator_loop_control`/`align_generator_for_of_stack`. The new `Parked` arm must be
  provably unreachable there (`unreachable!()`, not a silent fallback) — run the full sync-generator
  slice of test262 (`test262/test/built-ins/GeneratorPrototype/`, `test262/test/language/
  statements/generator*`) explicitly, not just trust the invariant argument in §2.3.
- **Async functions**: `close_for_of_loop`'s extraction (slice 1) must not change its observable
  behavior for `eval.rs`'s `unwind_for_of!`/`unwind_async_for_of_loops` callers. Covered by the full
  test262 run (async functions are a large fraction of it) plus `test262-extra/` files already
  pinned to async-function `for-await`/`await using` behavior (grep for existing
  `async-function-for-await-*` files from #737 and confirm they still pass unchanged).
  `test262/test/language/expressions/async-function/`, `test262/test/language/statements/async-
  function/`.
- **GC rooting**: the new `generator_pending_for_of_retry` table holds no `JsValue`s, so it needs
  no `collect_gc_roots` entry — but it does need `free_gc_object`/generator-retirement cleanup
  (§4), or a retired generator's id could collide with a later object reusing the same id and
  spuriously retry stale routing. Add a regression test for this specifically (create, abandon
  mid-park is not reachable since GC can't collect a parked generator — but retire-then-reuse-id is
  worth a `tests/` unit check if the id-reuse mechanism makes it plausible; confirm during
  implementation whether object ids are ever reused after a full GC cycle).
- **`test262-pass.txt` baseline**: none of this is expected to newly pass or newly fail any test262
  case (the fix changes timing, not final results) — a diff against the baseline should be empty.
  If the targeted run shows any file flipping pass/fail, treat that as a signal the completion
  value changed (a bug in the new code), not as progress to bank.
- **Bytecode fast path**: generators/async generators are tree-walker-only (`bytecode_enabled`
  gate, per CLAUDE.md's Architecture Notes) — no interaction expected; confirm no `bytecode/` file
  references `unwind_generator_for_of_loops`/`close_for_of_loop` before assuming this.
- **Property MOP (`property.rs`)**: not implicated — this plan touches no `[[Get]]`/`[[Set]]`/proxy
  trap code.

## 8. Out of scope

- **Item 2** (inline-yield-replay disposal, `is_inline_replay` in the frame-leave loop still uses
  blocking dispose at `generator_runtime.rs:3693-3737` for the "not `can_park`" branch). The issue
  makes this explicitly conditional: "re-check once the `InlineYield` fallback is removed (#729)."
  #729 (merged) made the fallback's own `Await`s suspend but did **not** remove the fallback —
  CLAUDE.md's Architecture Notes and ADR-2026-09-21-2157 both still describe
  `SentValueBindingKind::InlineYield`/`self.generator_context` as live, and #625 (closed) is an
  investigation into whether it's dead code, not a removal. The precondition the issue names for
  re-checking this item has not occurred. Fixing it now would mean solving "park mid-disposal
  during a replay whose own re-execution semantics are still the degraded fallback path" — a
  materially different and harder problem than parking during a clean re-entry, and one the issue
  author explicitly deferred. Recommend the follow-up issue track this as blocked on #625's
  resolution (retire the fallback, or decide it's permanent and re-scope item 2 against that).
- **Item 3** (delegated `yield*` abrupt exits skip enclosing `finally`/outer `for-of` closing;
  several reject/throw arms complete the generator without `DisposeResources` at all —
  `generator_runtime.rs:2764-2851` rejected/malformed inner result, `:3119-3126` `iterator_return`
  error, `:3215-3251` no-`.return()`-method arm, `:3271-3282` no-`.throw()`-method arm). This is a
  real, separate class of bug (missing `DisposeResources` calls, not just blocking ones — see the
  `has_catch`-only special case already present at `:2805-2843`, which shows the intended shape:
  deliver the delegation's completion as `pending_exception`/`pending_return` at
  `deleg_info.resume_state` with `delegated_iterator: None`, and let the ordinary `check_abrupt_on_
  resume` path — the same one item 1 makes resumable — run `finally`/for-of unwind/dispose
  correctly). It depends on item 1 landing first for its for-of/finally unwind to be non-blocking,
  touches ~6 call sites each with distinct reject-shape bookkeeping (queue pop, `retire_generator`
  timing), and changes the *rejection reason* observable to user code in some cases (a rejected
  inner result becomes a thrown-into-the-body completion, which a surrounding `catch` may now
  swallow instead of the request rejecting) — a behavior change worth its own PR and review, not a
  rider on this one. Recommend the follow-up issue carry this forward against item 1's landed
  primitives.
- **Async functions' equivalent blocking `iteration_env` dispose** in `close_for_of_loop` (§2.4) —
  real, pre-existing, shared, but outside `generator_runtime.rs` and not named by #733.
- **Refactoring `unwind_generator_for_of_loops`'s IteratorClose-on-normal-completion question**
  noticed in passing while reading `close_for_of_loop` (`align_generator_for_of_stack`'s `Goto`
  path calls it even for what looks like a normal, non-abrupt loop exit) — not a blocking-vs-
  suspending issue, not touched by this plan, and not confirmed as a real spec deviation; flag for
  a separate investigation only if it resurfaces.
- Any `test262-pass.txt` baseline update (main-branch-only operation).
- Formatting/style cleanup beyond what `./scripts/lint.sh` requires on touched lines.
