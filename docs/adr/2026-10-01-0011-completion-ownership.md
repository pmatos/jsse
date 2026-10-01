# Completion ownership: parking a running finally's throw/return/loop-control on its own try context

Issue #719, closing the gap PR #717 (fixing #689's regressions) explicitly
deferred. Commit c1f06903 (PR #740) first proved the fix for the sync
generator driver; this issue extends the same invariant to the async
generator and plain async-function drivers, which still parked an
intercepted completion on driver-global state.

## Problem

A running `finally`'s intercepted abrupt completion — a throw, a return, or
a lowered loop-control jump — was stored on the driver (an `IteratorState`
field, an `AsyncFunctionState` field, or a plain local), not on the specific
`TryContextInfo` whose finalizer was running it. A nested `try`/`finally`
entered inside that finalizer's own body popped its own context at its own
`TryExit`, but read the same unscoped driver state — so it could rethrow,
re-return, or resume the jump early, skipping the remainder of the outer
finally. Two sharper variants showed up once the fix was underway:

- The plain async-function driver's `EnterFinally` unconditionally
  overwrote `saved_finally_exception`, even on a finally entered by
  ordinary normal control flow (no throw of its own) — silently losing an
  enclosing finally's throw entirely, not merely misdelivering it.
- `route_return!` never truncated `try_stack` after routing to a finally
  (unlike `route_loop_control!`, which already did), so once its
  completion was moved onto the context, a later throw's own truncation —
  conditioned on "was something being replaced", detected via the
  driver-global slots that no longer held anything — silently stopped
  firing, leaving a completed inner context on the stack to mis-target the
  next `EnterFinally`.

## Decision

**Own the completion on the context, not the driver.** `TryContextInfo`
carries an allocation-free tagged `pending_completion: Option<PendingCompletion>`
(`Throw(JsValue)` / `Return(JsValue)` / `LoopControl(LoopControlTarget)`) —
a tagged enum, not three optional fields, because ECMAScript carries exactly
one Completion Record at a time and a conflicting pair should be
unrepresentable. Routing invariant, applied identically (but independently
implemented — see Non-unification below) across all three drivers:

1. `TryEnter` pushes a context with no pending completion.
2. A throw, return, or lowered loop-control jump uses its driver's existing
   handler-boundary and iterator/scope-unwind logic to find which `finally`
   it must run through.
3. **The interception site parks the completion on that resolved context
   and truncates `try_stack` to it, unconditionally** — not only when
   something is being replaced. Truncation is what structurally guarantees
   the next `EnterFinally` and `TryExit` land on the right context; a
   conditional truncate (gated on "was replacing something", as the
   async-function throw-routing block did) silently stops firing once
   nothing populates the condition's inputs anymore.
4. `EnterFinally` marks the context entered. For a throw, whose routing
   logic only knows which *state* to jump to, not which context will own
   it, `EnterFinally` is also where the handoff happens — it moves the
   driver's transient "exception in flight" value onto the context it just
   marked. For a return or loop-control jump, whose interception site (step
   3) already resolved the owning context directly, nothing further is
   needed at `EnterFinally`.
5. `TryExit` pops exactly its own context and matches on *its*
   `pending_completion` — `Throw`/`Return`/`LoopControl` each re-enter that
   same completion's existing routing/delivery logic, retargeted to read
   from the popped context instead of a driver-global slot; `None` falls
   through to the driver's pre-existing fallback paths (a fresh,
   not-yet-owned resume input — see below).
6. A new abrupt completion escaping a running finally replaces the older
   one structurally: it is routed (step 2) to a context whose depth is at
   or above the one holding the older completion, so truncating to the new
   context's depth (step 3) drops the old context — and the completion
   parked on it — together. A completion caught *inside* the finally never
   reaches step 2, so the context (and what it holds) survives untouched.

**Kept deliberately off the context: one-shot resume inputs and unwind
cursors.** `IteratorState::pending_exception`/`pending_return` (both
generator drivers) and the in-flight `pending_exception` local (async
functions) still exist, narrowed to exactly one role: a genuinely fresh,
externally-delivered `.throw()`/`.return()` injection, or an exception a
`Throw` operand just produced, in the brief window before it is actually
intercepted by `EnterFinally` or a return/loop-control interception site and
thereby becomes context-owned. `PendingDispose`, `PendingForOfUnwind`,
bindings, and delegation state are a different category entirely — unwind
cursors and obligations, not completions — and remain separate fields
untouched by this change.

**Non-unification.** The three drivers' own routing mechanics —
`route_generator_exception`/`route_generator_loop_control` (shared only
between the two generator drivers), `route_return!`/`route_loop_control!`
(async functions), and each driver's own disposal/iterator-closing/promise
plumbing — are not merged into one shared implementation. Only the
ownership invariant above, and the `TryContextInfo`/`PendingCompletion`
types it is expressed in, are shared.

## GC implications

A parked `JsValue` payload must be traced wherever a try stack carrying it
lives:

- **Suspended generator/async-generator state**: `collect_iterator_state_roots`
  already traces `TryContextInfo::pending_completion`'s payload
  unconditionally on `IteratorState::StateMachineGenerator`/
  `StateMachineAsyncGenerator`, regardless of `execution_state` — added once,
  by c1f06903, and reused as-is for every driver this issue touches.
- **Suspended async-function state**: `AsyncFunctionState` lives in the
  scheduler map rather than on an object; its own root loop needed the
  identical `try_stack` walk added explicitly (it does not fall out of the
  generator fix for free).
- **Actively executing (not yet suspended) state** is the gap the two
  points above miss: both the live `IteratorState` and the re-inserted
  `AsyncFunctionState` snapshot a `try_stack` once, at driver entry, before
  the state loop starts running — a park that happens *mid-loop* (the
  common case: `EnterFinally`'s throw handoff, or a return/loop-control
  interception site, both of which `continue` within the same driver
  invocation rather than crossing a function boundary) mutates the *local*
  `try_stack` without ever refreshing that snapshot. A `$262.gc()` called
  from directly inside the running finally body, before any
  yield/await/suspension, would only see the stale pre-park snapshot.
  Fixed by writing the current `try_stack` back at each such mid-loop park
  site: for generators, directly into the live object's `IteratorState` in
  place (no new side table needed, since the existing root walk above
  already traces that field unconditionally); for async functions, via the
  new `JobScheduler::sync_async_function_try_stack`, mirroring the pattern.
  One site turned out *not* to need this: the sync generator's own
  `generator_return_state_machine` hands off to a fresh
  `generator_next_state_machine` call, which re-serializes `try_stack`
  (now including the just-parked completion) into its own entry snapshot
  before any finally-body statement can run — a real function-boundary
  crossing, not a `continue`.

## Consequences

- `AsyncFunctionState::pending_return`/`pending_loop_control`/
  `saved_finally_exception`, and the generator drivers'
  `stays_inside_running_finally` compensating check (which cleared
  `pending_exception`/`pending_return` on a jump leaving a finally, on the
  since-obsolete theory that those locals might hold a parked completion),
  are deleted outright — not deprecated or kept behind a flag. Nothing has
  written a `Some` into any of them since the fix landed; keeping them
  would have been dead code pretending to be live state.
- `IteratorState::pending_exception`/`pending_return` are *not* removed.
  Their narrowed "fresh resume input" role is real and load-bearing;
  removing them is an independent, smaller follow-up, out of scope here.
- Six fix commits (one per driver × {throw, return}, with loop-control
  folded into the async-function return commit once its own bug surfaced)
  plus one GC-rooting commit, one cutover commit, and one coverage/comment
  commit — not three (one per driver) — because the throw and return halves
  of a single driver have independent interception and `TryExit`-read
  sites that regress if landed separately, and the async-function throw-
  routing block's conditional-truncate bug was discovered only by the
  return commit's own regression test, forcing it to land together with
  that commit rather than its originally-planned place in the cutover.
