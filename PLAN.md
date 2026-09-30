# Plan: issue #719 — pending exception/return can be consumed by the wrong TryExit in nested try/finally within a generator

> This plan spans three drivers across two files and is not expected to finish
> in one sitting. **Commit and push after every green slice** (§4), not just
> at the end — a slice is a reviewable, working unit on its own.

## 1. Problem restated

A running `finally`'s intercepted abrupt completion (a throw, a return, or a
loop-control jump) must be owned by the specific `TryContextInfo` whose
finalizer is running it, so that a nested `try`/`finally` entered inside that
finalizer's own body — or a suspension in between — cannot see or consume it.
Commit `c1f06903` (already on this branch) introduced `PendingCompletion`
(`Return`/`Throw`/`LoopControl`) on `TryContextInfo` and fixed exactly this for
the **sync generator** driver, with regression coverage added in `b75db7b5`.
Its commit message explicitly scoped out two remaining defects as follow-up:
the **async generator** driver and the **plain async-function** driver both
still park an intercepted throw or return in driver-local/state-global
variables instead of on the owning context. This plan closes both remaining
drivers plus the cutover the issue's acceptance criteria require, rather than
deferring either a second time — the issue's own acceptance criteria name
async-function `break`/`continue` and nested-throw cases explicitly, so an
async-generator-only PR would not close it.

## 2. Spec basis

- `sec-try-statement-runtime-semantics-evaluation` (`spec/spec.html:23182`):
  for `try Block Finally`, the Finally clause's completion replaces the
  Block's completion only if the Finally's own completion is abrupt; a
  normally completing `Finally` restores the Block's original completion via
  `UpdateEmpty`. This is the invariant the drivers violate: a normally
  completing nested `try`/`finally` inside a running `Finally`'s statement
  list must not disturb the completion that `Finally` is running for, and an
  abruptly completing one must replace it.
- `sec-generatorresumeabrupt` / `sec-asyncgeneratorresume` (`spec/spec.html:50365`,
  `:50749`): a generator resumption delivers exactly the Completion Record it
  was resumed with to the point where the generator is suspended. There is no
  spec-level notion of a completion "waiting" across a resumption beyond the
  try/finally restoration above; a driver's own scratch variables are an
  implementation detail that must not leak state across a resume they don't
  own.
- `sec-asyncgeneratorenqueue` / `sec-asyncgeneratorcompletestep`
  (`spec/spec.html:50699`, `:50716`): the async generator request queue
  delivers one Completion Record per resume; nothing in the abstract
  operations distinguishes "fresh" vs "parked" completions — that distinction
  is purely an engine-implementation artifact, which `PendingCompletion` on
  `TryContextInfo` exists to model correctly.
- Plain async functions have no dedicated resumption abstract operation beyond
  `Await` (`sec-await`) and ordinary `TryStatement` evaluation — the same
  `sec-try-statement-runtime-semantics-evaluation` clause governs completion
  ownership there; no additional spec text specific to async functions is
  needed.

No JavaScript syntax or semantics are being changed — this is an
engine-internal completion-routing bug fix; the observable behavior it
produces (matching Node and the clauses above) is already dictated by them.

## 3. Files to touch

- **`src/interpreter/eval/generator_runtime.rs`** — the async generator
  driver, extending the already-proven `c1f06903` pattern:
  - `StateTerminator::EnterFinally` in `async_generator_next_state_machine_impl`
    (currently `generator_runtime.rs:4599-4604`): move an intercepted
    `pending_exception` onto `ctx.pending_completion` before entering the
    finally body, mirroring the sync arm at `generator_runtime.rs:1537-1550`.
  - `StateTerminator::TryExit` in the same function (currently
    `generator_runtime.rs:4554-4576`): stop reading the driver-local
    `pending_exception`/`pending_return` directly (lines 4556, 4561). Pop the
    context first and match on `finished.pending_completion` for
    `Throw`/`Return`/`LoopControl`/`None`, mirroring the sync arm at
    `generator_runtime.rs:1467-1514` — keep each arm's existing body
    (`route_exception!`, `dispose_or_park!`, `reject_async_generator_request`,
    already used nearby for the same purpose, e.g. `generator_runtime.rs:4636-4640`)
    verbatim, changing only where the matched value comes from. Minimizing the
    change to "retarget the input" rather than restructuring the arm sidesteps
    any question of whether disposal now runs twice.
  - The pending-return interception block inside the same function's
    `check_abrupt_on_resume` handling (currently `generator_runtime.rs:3619-3658`)
    and `async_gen_reenter` (`generator_runtime.rs:5404-5448`, which is what
    actually writes an externally-delivered `.return()`/`.throw()` back onto
    the stored `IteratorState` after `Await`ing the operand): when an
    enclosing finally is found, park the return directly on
    `current_try_stack[idx].pending_completion = Some(PendingCompletion::Return(..))`
    instead of the driver-local `pending_return = Some(return_value)` at line
    3654, mirroring the sync fix at `generator_runtime.rs:2142-2148`.
  - A small shared helper for "find the next un-entered enclosing finally,
    unwind for-of loops up to it, and either park onto that context or
    finish" — both the `check_abrupt_on_resume` return-interception block and
    the new `TryExit` Return arm need the same logic; factor it out rather
    than duplicating (exact shape is the implementer's call).
- **`src/interpreter/eval.rs`** — the plain async-function driver
  (`async_function_resume`, `eval.rs:8184-9696`). Every case below needs an
  `await` somewhere in the `try` or `finally` body to force the transform to
  lower it to this state machine at all — a synchronous async function
  without a suspension point can still be evaluated by a simpler blocking
  path that never touches `try_stack`/`route_return!`/`route_loop_control!`.
  If a test is green without an `await` present, that's a false negative, not
  evidence the bug is absent.
  - `StateTerminator::EnterFinally` (`eval.rs:9239-9246`): currently
    `saved_finally_exception = pending_exception.take()` **unconditionally**.
    This is a sharper bug than the issue's own description: because every
    `try`/`finally` (not just an abruptly-entered one) routes through
    `EnterFinally` on its normal-completion path too, a nested try/finally
    entered by plain sequential control flow inside a running outer finally
    silently overwrites `saved_finally_exception` with `None`, losing the
    outer throw entirely rather than merely misdelivering it early. Fix: move
    an intercepted `pending_exception` onto `ctx.pending_completion` instead
    of the single driver-global slot, the same as the two generator drivers.
  - `StateTerminator::TryExit` (`eval.rs:9187-9213`): currently checks four
    unscoped slots in sequence (`pending_exception`, `pending_return`,
    `saved_finally_exception`, `pending_loop_control`). Pop the context and
    match on `finished.pending_completion` instead, keeping each existing arm
    body verbatim (same "retarget the input only" approach as the async
    generator driver above). Keep the *in-flight, not yet routed*
    `pending_exception` re-check and the `pending_for_of_unwind` clearing as
    separate concerns — they are the "one-shot resume input" and "unwind
    cursor" categories the issue distinguishes from context-owned
    completions; do not fold them in.
  - `route_return!` macro (`eval.rs:8456-8517`): delete the unconditional
    `pending_loop_control = None` at line 8461 (a return produced by a
    finalizer legitimately replaces an in-flight loop-control completion, but
    only when the return actually leaves that finalizer — see the truncation
    point below, which is what makes that true structurally instead of by a
    separate clear). Park the value on `try_stack[i].pending_completion`
    instead of the driver-local `pending_return = Some(ret_val)` at lines
    8504-8506, and **truncate `try_stack` to `i + 1` unconditionally** at the
    same point — this macro currently never truncates at all (contrast
    `route_loop_control!`, which already truncates at line 8583). Per the
    issue's design invariant (step 3: "When a finalizer intercepts a
    completion, truncate to that context, store the completion on it, and
    enter its finalizer"), truncation is not conditional on anything — it is
    what makes `EnterFinally`'s `try_stack.last_mut()` land on the correct
    context and what makes `TryExit` pop the correct one next.
  - `route_loop_control!` macro (`eval.rs:8524-8590`): delete the
    unconditional `pending_return = None; saved_finally_exception = None;` at
    lines 8531-8532 with **no replacement conditional check**. This macro
    already truncates correctly at line 8583, mirroring the already-correct
    shared `route_generator_loop_control` (`generator_runtime.rs:6027-6082`,
    context-parking at line 6069) — once a loop-control jump's own truncation
    only drops contexts up to the finally it actually routes through, a jump
    whose target lies inside that same finally never truncates past it, so
    that finally's `pending_completion` (if any) survives automatically. Do
    not add a `stays_inside_running_finally`-style conditional here to
    compensate for the deleted clear; per the issue's design, truncation
    alone is the invariant, and this bug is exactly the case of a clear
    substituting for correct ownership. (The generator drivers' own
    `stays_inside_running_finally` — `generator_runtime.rs:12-17`, called at
    lines 1374 and 4478 — is the same kind of compensating check and becomes
    dead weight for the same reason once its callers stop clearing
    `pending_exception`/`pending_return` for anything but a genuinely fresh
    resume input; see the cutover slice.)
  - The throw-routing block at the top of the dispatch loop
    (`eval.rs:8760-8889`): `pending_return_was_replaced`/
    `pending_loop_control_was_replaced`/`pending_completion_was_replaced`
    (lines 8786-8789) gate the `try_stack.truncate(depth + 1)` at line
    8882-8886 so it only runs when something was being replaced. Per the same
    invariant, truncation to the selected handler's depth must happen
    **unconditionally** whenever a finally handler is selected (`is_catch ==
    false`), not just when replacing an existing return/loop-control — remove
    the gate and always truncate. This collapses the need for the three
    boolean flags; verify nothing else downstream reads them before deleting.
  - `async_fn_suspend_at_await` (`eval.rs:9719-9769`) and its call sites
    (approx. `8410-8427`, `8666-8683`, `9043-9045`, `9082-9084`, `9110-9112`,
    `9432-9434`): stop threading `pending_return`/`saved_finally_exception`
    through every suspension point once they're removed from
    `AsyncFunctionState`; `try_stack` (already threaded) now carries
    everything they used to.
- **`src/interpreter/types.rs`**:
  - Remove `AsyncFunctionState::pending_return`, `pending_loop_control`,
    `saved_finally_exception` (declared at `types.rs:400-402`) once nothing in
    `eval.rs` reads them.
  - `IteratorState::StateMachineGenerator`/`StateMachineAsyncGenerator`'s own
    `pending_exception`/`pending_return` fields (`types.rs:1480-1481`,
    `1492-1493`) are **not** removed by this plan. Once the interception fixes
    above land, their only remaining live role is carrying a genuinely fresh,
    externally-delivered `.throw()`/`.return()` completion into the next
    resume (the "one-shot resume input" category the issue names as
    legitimately distinct from context ownership) — document that narrowed
    role in the new ADR (below) rather than claiming removal here; removing
    them is a separate, smaller follow-up once it's independently confirmed
    nothing else still writes them for parking purposes.
- **`src/interpreter/gc.rs`**:
  - `collect_iterator_state_roots` (`gc.rs:1079-1145`) already traces
    `try_info.pending_completion`'s `JsValue` payload generically for both
    generator variants (added by `c1f06903`) — no change needed there for the
    already-suspended case.
  - The `AsyncFunctionState` root loop (`gc.rs:468-485`) currently roots
    `afs.pending_return` (472-474) and `afs.saved_finally_exception` (480-482)
    directly; once those fields are removed, replace both with a loop over
    `afs.try_stack` rooting each `TryContextInfo::pending_completion`'s
    payload, mirroring `gc.rs:1134-1141` exactly. Note this loop only sees
    *suspended* async-function states (`self.scheduler.iter_async_function_states()`);
    see the next bullet for the actively-executing case, which this one does
    not cover.
  - **New: GC safety for a completion parked during active (unsuspended)
    execution.** A completion parked on `ctx.pending_completion` is reachable
    only through the driver's local `try_stack`/`current_try_stack` variable
    from the moment it's parked until either the next suspension serializes
    that stack back into persistent state, or `TryExit` consumes it.
    Arbitrary finally-body code runs in between, and any allocation can
    trigger a GC. This codebase already has an established pattern for a
    driver's actively-executing local stacks: `generator_for_of_stacks`/
    `generator_scope_stacks` (`HashMap<u64, Vec<_>>` side tables, rooted
    unconditionally in `collect_gc_roots` at `gc.rs:450-457`, regardless of
    suspension state) are kept in sync with the local `for_of_stack`/
    `scope_stack` variables via `sync_generator_for_of_stack`/
    `sync_generator_scope_stack` calls scattered through both generator
    drivers. Mirror this for `try_stack`: add an equivalent side table (e.g.
    `generator_try_stacks`), sync it at the same cadence, and root it the same
    way. For the async-function driver, `AsyncFunctionState` (which owns
    `for_of_stack`/`scope_stack` today) is removed from the scheduler map
    while it's actively executing (`async_function_resume` takes it out at
    entry and only reinserts at suspension) — check first whether
    `afs.for_of_stack`/`afs.scope_stack` already have this exact exposure gap
    during active execution before inventing a new mechanism for `try_stack`
    alone; if they do, this is pre-existing and out of this issue's scope to
    fully close, but `try_stack` should get whatever the least-new-machinery
    fix is (a matching side table, or extending an existing one if for-of/
    scope already got one). Do not use `with_gc_root_scope`
    (ADR-2026-09-10-2014) for this — it is a single-call closure combinator
    and cannot span the many separate state-machine loop iterations between
    `EnterFinally` and `TryExit`.
- **`src/interpreter/mod.rs`** — remove the three `AsyncFunctionState` field
  initializations at approx. `mod.rs:3929-3931` alongside the `types.rs`
  removal.
- **`test262-extra/`** — new regression files for the async generator and
  async-function drivers, mirroring the existing sync-generator suite from
  `b75db7b5` (see §5).
- **`CONTEXT.md:105`** — currently states the generator drivers "park the jump
  on the finalizer's `TryContextInfo.pending_loop_control`"; that field was
  replaced by `TryContextInfo.pending_completion` (holding
  `PendingCompletion::LoopControl`) in `c1f06903` and the sentence was never
  updated. Fix it, and extend it to describe `Throw`/`Return` routing the same
  way across all three drivers post this plan.
- **`docs/adr/`** — add a new ADR recording: extending the `c1f06903`
  ownership model to the async generator and async-function drivers via
  unconditional truncation-at-interception as the single invariant (replacing
  the ad hoc replace/priority booleans and `stays_inside_running_finally`);
  the `check_abrupt_on_resume` root cause; the narrowed "resume input only"
  role left for `IteratorState::pending_exception`/`pending_return`; and the
  GC side-table requirement for completions parked during active execution.
  No existing ADR covers this topic — confirmed by grepping every file under
  `docs/adr/` for `689`, `717`, `719`, `PendingCompletion`, `TryContextInfo`,
  and `check_abrupt_on_resume` (zero hits); the closest prior art is the
  `2026-09-2x` async-generator disposal/suspension ADR series, none of which
  address completion *ownership*.

## 4. TDD slices

Throw and return each need their *interception* site and their `TryExit`
*read* site changed together per driver — landing one half alone regresses
(moving a throw onto `ctx.pending_completion` at `EnterFinally` without also
teaching `TryExit` to read it there means the throw is silently dropped at the
next `TryExit`, since the old `pending_exception.take()` there would now find
`None`). Each slice below keeps both halves together. **Commit and push after
each numbered slice goes green.**

1. **Async generator: throw ownership.** Red: add
   `test262-extra/async-generator-nested-finally-preserves-outer-pending-throw.js`
   (async analogue of `generator-nested-finally-preserves-outer-pending-throw.js`,
   driven via `asyncTest`/`asyncHelpers.js`), and
   `test262-extra/async-generator-throw-after-multiple-finally-yields.js`
   (the issue's third confirmed failure: `try { throw 'E'; } finally { yield 1; yield 2; }`
   must yield 1, yield 2, then reject with `E`). Confirm both fail on the
   current tree before changing code. Green: apply the `EnterFinally` move and
   the `TryExit` Throw arm together (§3). The second test needs no separate
   mechanism — it passes once the driver stops leaving a stale value in the
   local `pending_exception` for `check_abrupt_on_resume` to misfire on at the
   next resume.
2. **Async generator: return ownership.** Red: add
   `test262-extra/async-generator-nested-finally-preserves-outer-pending-return.js`,
   covering both a body-level `return` and an external `.return()` (they hit
   different code paths — `StateTerminator::Return`'s finally-lookup at
   `generator_runtime.rs:4223-4248` vs. `async_gen_reenter`/
   `async_generator_return_state_machine_with_promise`). Confirm red first.
   Green: apply the return-interception park and the `TryExit` Return arm
   together (§3).
3. **Async generator: `yield*` preserves an outer pending return.** Red: add
   `test262-extra/async-generator-yield-star-preserves-outer-pending-return.js`
   (async analogue of `generator-yield-star-preserves-outer-pending-return.js`).
   Every `delegated_iterator: Some(...)` construction site in
   `generator_runtime.rs` (lines 723, 1271, 2011, 2343, 2984, 3030, 3051, 4001)
   already clones `try_stack` alongside it, so this is expected to pass as a
   side effect of slices 1-2, the same way it did for sync in `c1f06903` —
   write the test expecting green; if it's red, that's new information
   requiring a fix at whichever site drops `try_stack`, not an assumption to
   carry into the plan.
4. **Async generator: override matrix and suspended-state GC rooting.** Red:
   add `test262-extra/async-generator-pending-completion-override-matrix.js`
   and `test262-extra/async-generator-pending-completion-gc-rooting.js`
   (async analogues of the two sync files from `b75db7b5`). Expected green
   from slices 1-2 (override behavior) and from the existing generic GC
   tracing in `collect_iterator_state_roots` (rooting while suspended) — this
   slice proves both rather than assuming them.
5. **Async function: throw ownership.** Red: add
   `test262-extra/async-function-nested-finally-preserves-outer-pending-throw.js`
   (`test262-extra/`, per this project's convention for spec-correct behavior
   test262 doesn't cover — not `tests/`) reproducing both: (a) an outer throw
   with a nested, *abruptly* entered try/finally inside the finally body, and
   (b) the sharper bug found while reading `EnterFinally` — an outer throw
   with a nested try/finally entered by plain *normal* sequential control
   flow (no throw/return of its own) inside the finally body, which today
   silently loses the outer throw entirely. Every case needs an `await`
   somewhere in the try or finally (§3) so the body actually lowers to the
   state machine. Confirm both red. Green: apply the `EnterFinally` fix (§3).
6. **Async function: return ownership, including the `route_return!`
   truncation bug.** Red: add
   `test262-extra/async-function-nested-finally-preserves-outer-pending-return.js`
   with a case with a `return` intercepted by an outer finally containing a
   nested, normally-completing try/finally, and a case with
   `while (true) { await 0; break; }`/`continue` inside a suspending
   finalizer whose return the issue names directly. Confirm red. Green: apply
   the `TryExit` Return-arm rewrite plus the unconditional `route_return!`
   truncation fix together (§3).
7. **Async function: loop control preserves an outer completion it stays
   inside of, and replaces one it leaves.** Red: add
   `test262-extra/async-function-loop-control-preserves-outer-pending-completion.js`
   with (a) a `break`/`continue` targeting a label *inside* the same running
   finally that's carrying an outer return or throw (today's unconditional
   clear at `eval.rs:8531-8532` wipes it even though the jump never leaves the
   finalizer), and (b) one that *does* leave the finalizer, confirming it
   still correctly replaces the earlier completion (via truncation, not a
   clear). Green: delete the unconditional clear with no replacement
   conditional (§3).
8. **Async function: override matrix and suspended-state GC rooting.** Red:
   add `test262-extra/async-function-pending-completion-override-matrix.js`
   (return→return, return→throw, throw→return, throw→throw, a throw caught
   inside the finalizer preserving the earlier completion, an uncaught one
   replacing it, and loop control leaving the finalizer replacing the earlier
   completion) — this is the async-function analogue the plan was missing
   even though §6 flags the throw-routing replace/priority block
   (`eval.rs:8760-8889`) as this plan's riskiest single piece, and
   `test262-extra/async-function-pending-completion-gc-rooting.js` (an
   external caller forces `$262.gc()`, `features: [host-gc-required]`, while
   the async function is suspended at an `await` inside the finally that owns
   the parked completion — exercises the new `afs.try_stack` GC root loop
   from §3). Expected green from slices 5-7 and the `types.rs`/`gc.rs`
   changes; this slice proves both rather than assuming them.
9. **GC safety for a completion parked during active (unsuspended)
   execution.** Red: for each of the three drivers, a test with a bare
   `$262.gc()` **statement written directly inside the running finally body**,
   before any `yield`/`await` — e.g.
   `function* g() { try { throw {marker:'X'}; } finally { $262.gc(); yield 1; } }`
   — so the forced collection runs while the throw is parked only on the
   local, not-yet-serialized `try_stack`, matching the existing
   `features: [generators, host-gc-required]` / `$262.gc()` style already used
   by `generator-pending-completion-gc-rooting.js`, not a disposer or
   `valueOf` hook. This is genuinely new coverage — the existing sync test
   only forces GC after a suspension, once the value is already serialized
   into rooted state. If red, add the side-table fix from §3; do not assume
   this is already safe just because the suspended case is covered.
10. **Cutover: remove obsolete state and dead compensating checks.** Once
    slices 1-9 are green:
    - Delete `AsyncFunctionState::pending_return`/`pending_loop_control`/
      `saved_finally_exception` (`types.rs:400-402`), their initializers
      (`mod.rs:3929-3931`), and their GC roots (`gc.rs:472-482`), replacing
      the latter with the `try_stack`-based loop (§3).
    - Delete `stays_inside_running_finally` (`generator_runtime.rs:12-17`)
      and its call sites (lines 1374, 4478) — once `pending_exception`/
      `pending_return` in both generator drivers only ever hold a fresh
      resume input (never a parked completion), a `break`/`continue`
      statement has no reason to touch them at all.
    - Delete the now-unused `pending_return_was_replaced`/
      `pending_loop_control_was_replaced`/`pending_completion_was_replaced`
      locals in `eval.rs`'s throw-routing block once truncation there is
      unconditional (§3).
    Green: the full suite from slices 1-9 stays green with all of the above
    gone — this slice is deletion with no new behavior, proven by the absence
    of new failures, not new tests.
11. **Full regression run.** `uv run python scripts/run-test262.py` (no path
    filter) against the baseline from `origin/main:test262-pass.txt`, plus
    `cargo test --release`. The gate before calling the PR done, not a
    red/green slice on its own.

## 5. Test surface

- Targeted test262 directories to re-run after the change: `test262/test/language/statements/try/`,
  `test262/test/language/statements/generators/`,
  `test262/test/language/expressions/generators/`,
  `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/`,
  `test262/test/built-ins/AsyncGeneratorFunction/`,
  `test262/test/built-ins/AsyncGeneratorPrototype/`,
  `test262/test/language/statements/async-function/`,
  `test262/test/language/expressions/async-function/`.
- New `test262-extra/` files: the ten test-bearing slices above (all in
  `test262-extra/`, per this project's convention — see slice 5). The
  async-generator ones follow the existing sync-generator suite added in
  `b75db7b5` file for file, adapted to the async, promise-driven resumption
  model (`asyncTest`, `asyncHelpers.js`, matching the existing style, e.g.
  `async-generator-await-using-block-in-finally-with-inflight-completion.js`).
  This is spec-correct behavior not covered by test262 today (test262 has no
  case exercising nested try/finally completion ownership inside a running
  generator or async-function finalizer).
- `cargo test --release` — runs the existing unit tests in
  `src/interpreter/types.rs` (`completed_state_machine_generator_tests`),
  unaffected since the generator iterator's `pending_exception`/
  `pending_return` fields are not being removed in this plan (§3).

## 6. Regression risk

- **Shared routing helpers.** `route_generator_exception` and
  `route_generator_loop_control` (`generator_runtime.rs:5957`, `:6027`) are
  shared verbatim between the sync and async generator drivers. This plan
  does not change either helper — only the async driver's call sites that
  decide what to do with the result of routing. Run the full sync generator
  test262-extra suite from `b75db7b5` alongside the new async and
  async-function suites to confirm sync behavior (already fixed and tested)
  doesn't move.
- **`check_abrupt_on_resume` gate.** After this fix, it should never observe
  a non-`None` value that originated from a completion still owned by a
  running finalizer — only genuinely fresh external `.throw()`/`.return()`
  injections should reach it. Any test262 async-generator case relying on an
  external `.throw()`/`.return()` being delivered promptly at the next resume
  (the gate's own legitimate fast path) must keep passing; exercised by
  `test262/test/language/statements/async-generator/` and the existing
  `async-generator-await-*` test262-extra files.
- **`route_return!`/`route_loop_control!` truncation and clearing changes.**
  These macros are the async-function driver's most heavily-shared routing
  code (5 call sites for `route_loop_control!` alone). Making truncation
  unconditional and deleting the compensating clears is the highest-blast-
  radius part of this plan — run every existing `for-of`, `await using`, and
  scope-crossing async-function test262-extra file (the `await-using-*`/
  `for-of-*` suites referenced by ADR-2026-09-21-1007 and
  ADR-2026-09-21-2015) in addition to the targeted directories above, not
  just the new tests from slices 5-8.
- **The throw-routing replace/priority block (`eval.rs:8760-8889`).** Making
  its truncate unconditional and deleting the three boolean flags is a
  behavior-preserving simplification only if every existing case that
  currently relies on the conditional truncate is re-derived correctly from
  unconditional ownership — keep slice 8's override matrix and this block's
  own targeted re-run tight before moving to the cutover slice.
- **`yield*` delegation snapshots.** Relies on `try_stack` already being
  cloned into every delegation snapshot for async generators, the same as it
  was for sync before `c1f06903` (verified by direct reading — see §3's line
  list). If wrong, slice 3 surfaces it directly.
- **GC rooting.** Three independent claims to verify, not assume: (a) the
  already-suspended generator case is covered by existing generic tracing
  (verified by reading `collect_iterator_state_roots`); (b) the
  already-suspended async-function case needs the new `afs.try_stack` root
  loop (§3, slice 8); (c) the actively-executing case for all three drivers
  (§3, slice 9) likely needs a new side table and is not covered by anything
  existing.
- **Library/harness suites.** None of `decimal.js`, `big.js`, `acorn`,
  `prismjs`, `uglify-js`, `highlight.js`, `uuid`, `luxon`, `zod`, `moment` are
  known to exercise nested try/finally completion ownership in generators or
  async functions in a way this change would perturb; no targeted re-run
  planned beyond the full test262 baseline comparison in slice 11.
- **Bytecode fast path / property MOP.** Unaffected — this change is entirely
  within the tree-walker's state-machine drivers, not `eval_expr`/
  `exec_statement` dispatch or `property.rs`.

## 7. Out of scope

- **Unifying the three drivers' routing into one shared implementation.** The
  issue explicitly says not to: "Do not unify the three full drivers; their
  queue, disposal, iterator-closing, and promise mechanics legitimately
  differ. Share only the completion-routing invariant and small stack
  helpers." This plan applies the same *invariant* (unconditional truncation
  at interception) to all three but keeps each driver's own macros/functions
  (`route_return!`/`route_loop_control!` for async functions;
  `route_generator_exception`/`route_generator_loop_control`, shared only
  between the two generator drivers, for generators).
- **Removing `IteratorState::pending_exception`/`pending_return`.** Per §3,
  their narrowed "fresh resume input" role survives this plan; removing them
  entirely is a smaller, independent follow-up once that's confirmed safe in
  isolation, not bundled here.
- **`just_routed`** (`generator_runtime.rs:3445`, used at 3490, 3656, 3706)
  exists to distinguish "just unwound to a handler" from "carried through a
  running finally" — a distinction this plan's ownership model makes
  structural instead of a transient flag. Read its remaining uses during
  implementation; if it becomes redundant with `pending_completion`'s
  presence, removing it is in scope for *this* plan (it's the same
  conflation, not a separate concern), but do not restructure it speculatively
  before confirming which of its checks `pending_completion` already
  subsumes.
- **The pre-existing GC exposure of `AsyncFunctionState::for_of_stack`/
  `scope_stack` during active execution**, if slice 9 finds it already exists
  independent of this issue's `try_stack` work. Note it (a comment pointing to
  a new issue is enough); fixing it beyond what `try_stack` needs is not part
  of closing #719.
- **Rewriting `check_abrupt_on_resume` into a different mechanism entirely**
  (e.g. routing everything through `TryExit` unconditionally). Its role as
  the entry point for a genuinely fresh external `.throw()`/`.return()` is
  correct and spec-required; this plan only stops it from misfiring on a
  completion it doesn't own.
- **Formatting/unrelated cleanup** anywhere in `generator_runtime.rs` or
  `eval.rs` beyond the call sites and macros listed in §3.
