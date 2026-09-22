# Async generators: `for-of` unwind disposal suspends on the throw-routing path

Issue #733 (split from #716, following ADR-2026-09-22-2326's "known
boundaries"). Scopes down to item 1 of that issue — the `for-of` unwind
blocking bug — for its structurally-clean, well-precedented slice: the
`route_generator_exception` caller. Items 2 and 3, and the remaining
`for-of`-unwind call sites, are tracked in a follow-up issue (see below).

## Decisions

- **`unwind_generator_for_of_loops` takes a `can_park: bool` and returns a new
  `ForOfUnwindOutcome::{Done(Completion), Parked { cursor, value }}`.** A new
  per-env helper, `dispose_env_for_for_of_unwind`, is the single place that
  decides whether a resource's `DisposeCursor` blocks (`can_park = false`:
  drains the job queue inline via `run_dispose_cursor_blocking`, today's
  behavior, unchanged) or steps once and reports `Parked` on
  `DisposeStep::Await` (`can_park = true`). `dispose_scopes_inside_for_of`
  (the scopes nested inside a closing loop) takes the same flag and shares the
  same per-env helper, so both disposal points a closing loop can suspend at
  — its own `iteration_env` and any nested `await using` block scope — are
  covered uniformly.
- **A parked loop stays on `for_of_stack` with `iteration_env` already
  taken.** The unwind loop no longer pops a loop's `ForOfLoopState` before
  disposing it: it disposes nested scopes, then the loop's own
  `iteration_env` (via `Option::take`), and only pops and runs the
  synchronous `close_for_of_iterator` (see below) once both steps finish
  without parking. `try_stack` is truncated to the closing loop's `try_depth`
  *before* either disposal step runs, so a park always snapshots the
  post-truncation stack. This makes a second pass through the same unwind
  after resume idempotent: the loop is still on the stack, its
  `iteration_env` is `None` (nothing left to dispose there), any nested
  scope's `dispose_stack` was already taken by `take_dispose_stack` on the
  first pass (nothing left to dispose there either), `try_stack.truncate`
  to the same depth is a no-op, and the pass falls straight through to
  `close_for_of_iterator` — IteratorClose is synchronous per spec for a
  non-`for-await` loop and runs exactly once.
- **`close_for_of_loop` splits into itself (blocking `iteration_env` dispose,
  unchanged, for its sync-generator and async-function callers) plus a new
  `close_for_of_iterator`** taking a loop_state whose `iteration_env` is
  already `None`. Pure extraction (no behavior change for existing callers);
  the async-generator unwind calls `close_for_of_iterator` directly once its
  own resumable `iteration_env` dispose finishes.
- **`route_generator_exception` gains the same `can_park` flag and a new
  `RouteExceptionOutcome::{Routed, Throw(JsValue), Exit(i32), Parked { cursor,
  value }}`,** replacing its old `Completion`-typed return (`Completion::Empty`
  meant "handler found," reusing an unrelated completion kind as a sentinel —
  the new enum names the four outcomes directly). The async generator driver's
  `route_exception!` macro passes `can_park = true` and, on `Parked`, snapshots
  `execution_state: SuspendedAtState { state_id: current_id }` (the state
  that was executing when the exception was thrown — inert here, since
  `check_abrupt_on_resume`'s `pending_exception` branch re-routes from
  scratch before ever dispatching a state body) with `pending_exception` and
  `pending_return` both `None` — the cursor itself carries the in-flight
  throw, exactly the "don't double-book the completion" rule
  ADR-2026-09-22-2326 established for the frame-leave loop — then parks via
  `park_async_gen_disposal` with `GeneratorDisposeThen::Reenter`. `Reenter`
  restarts `async_generator_next_state_machine_impl` from the top;
  `check_abrupt_on_resume` sees the restored `pending_exception` (a disposer's
  own error, chained via `wrap_suppressed_error`, or the original throw
  unchanged) and re-invokes `route_exception!`, which — per the idempotency
  argument above — completes the unwind on this second pass. The sync
  generator driver (`generator_next_state_machine_impl`'s own
  `route_exception!` macro) and the direct `.throw()`-on-a-suspended-generator
  call site (`generator_throw_state_machine`) both pass `can_park = false` and
  treat `Parked` as `unreachable!()`: a sync generator's dispose stacks only
  ever hold `using` resources (never `await using`, a syntax error outside
  async contexts), and `AddDisposableResource` never even pushes a null/
  undefined *sync*-hint resource onto the stack (only a null/undefined
  *async*-hint one, to preserve `await using`'s single trailing tick) — so
  `DisposeCursor::step` provably never returns `DisposeStep::Await` for a
  sync caller's stack. This is verified by test262 (`GeneratorPrototype/`,
  `language/statements/generators/`), not merely assumed.
- **Every other `unwind_generator_for_of_loops` caller passes
  `can_park = false`** (`route_generator_loop_control`, the `pending_return`
  block, the two `Return` terminator arms, `align_generator_for_of_stack`,
  the sync-generator `.return()` path) and unwraps `ForOfUnwindOutcome::Done`,
  asserting `Parked` unreachable. This is mechanical plumbing to keep the
  signature change compiling, not a behavior change: today's blocking
  dispose is preserved for these call sites verbatim.

## Verification

`test262-extra/async-generator-for-of-throw-unwind-suspends.js`: a
witness-chain probe (pre-queued `Promise.resolve().then()` reactions, whose
firing position pins how many ticks a disposal consumed) covering a single
loop, nested loops, a disposer that itself throws (asserting
`SuppressedError` chaining, not just that *something* rejects), and a throw
caught by an enclosing `try`. All four assert the driver returns control to
its synchronous caller before draining any queued job — red before this
change (verified against a pre-change binary snapshot), green after.

## Known boundaries (not fixed here)

- **Item 2** (inline-yield-replay disposal): unchanged; the issue's own text
  defers this to #625 (whether the `InlineYield` fallback is ever retired).
- **Item 3** (delegated `yield*` abrupt exits skip enclosing `finally`/outer
  `for-of` closing, and several reject/throw arms complete the generator
  without `DisposeResources` at all): unchanged. It depends on this ADR's
  primitives (for-of/finally unwind must be non-blocking before it can run
  underneath a delegation's completion) and is a larger, separately-reviewed
  behavior change (a rejected inner result becomes a thrown-into-the-body
  completion, observable to a surrounding `catch`).
- **`route_generator_loop_control` (`break`/`continue` crossing a `for-of`)
  and `align_generator_for_of_stack` (`Goto`, normal loop exhaustion) still
  block.** Both lack a natural resume carrier the way a throw/return's
  completion is — closing the gap needs a new side table to remember what to
  redo after a park, plus a symmetric resume-time check alongside
  `check_abrupt_on_resume`. Sketched in the follow-up issue.
- **The `pending_return` block and the two `Return` terminator arms still
  block**, despite having a natural carrier (unlike loop-control), simply for
  scope: this ADR converts one caller end-to-end as the minimal vertical
  slice, and defers the rest.
- **Async functions' `close_for_of_loop` `iteration_env` dispose is still
  blocking** (`eval.rs`'s `unwind_for_of!`/`unwind_async_for_of_loops`); out
  of scope — `PendingForOfUnwind` solves a different problem (sequencing an
  abrupt completion through intervening handlers across a suspension that
  already happened elsewhere), not making this dispose itself resumable.
- A `for (await using …)` nested in a container with no `await`/`yield` of
  its own, named as a boundary in ADR-2026-09-22-2326, turned out to already
  be fixed by #737's `stmt_contains_for_of_head` recursion fix (merged before
  this branch); `test262-extra/async-generator-for-of-await-using-nested-in-container-suspends.js`
  locks in the now-correct behavior for both async generators and async
  functions. No production change was needed.

Follow-up issue: #742.
