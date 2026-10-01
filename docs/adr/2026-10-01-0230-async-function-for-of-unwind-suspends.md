# Async functions: `for-of` unwind disposal suspends (follow-up to #742)

Issue #779, split from #742 item 4. ADR-2026-09-22-2340 fixed the
async-generator driver's `for-of` unwind blocking bug across every caller
(extended by #761); its own "known boundaries" section explicitly left the
async-function driver's three call sites (`unwind_for_of!`'s `route_return!`
and `route_loop_control!` uses, plus the inline `Completion::Break` fast
path, and `unwind_async_for_of_loops`) still blocking. This ADR closes that
gap by reusing the exact same primitives rather than inventing a parallel
mechanism.

## Decisions

- **`ForOfUnwindOutcome` and `dispose_env_for_for_of_unwind` become
  `pub(super)`** (`generator_runtime.rs`) so `eval.rs`, their parent module,
  can call them directly instead of duplicating the block-vs-park decision.
  No behavior change to the generator side; `can_park: true` is passed from
  every async-function call site, exactly as the async-generator driver
  already does.
- **`unwind_async_for_of_loops` changes its return type from `Completion` to
  `ForOfUnwindOutcome`,** mirroring `unwind_generator_for_of_loops`'s own
  shape: the loop being closed stays on `for_of_stack` with its
  `iteration_env` taken via `Option::take` until that environment's own
  disposal finishes; only then does it pop and run the synchronous
  `close_for_of_iterator` tail. Its single call site (the top-level
  `pending_exception` throw-routing block) handles `ForOfUnwindOutcome::
  Parked` with the same gc-root + `async_fn_suspend_at_await` +
  `park_async_function_dispose` + `return` sequence `unwind_scopes_to!`
  already uses, parking with a new `DisposeThen::ForOfCrossThrow`.
- **`unwind_for_of!` generalizes from `($from:expr)` to `($from:expr,
  $seed:expr, $then:expr)`,** mirroring its sibling `unwind_scopes_to!`'s own
  seed/tag signature instead of always starting from `Completion::Empty`.
  Each level's `iteration_env` dispose goes through
  `dispose_env_for_for_of_unwind`/`can_park: true` the same way; on
  `ForOfUnwindOutcome::Parked` the macro parks with `$then` and returns, on
  `Done` it carries the resulting completion into `close_for_of_iterator`
  and the existing per-level handler-boundary re-check, which now runs after
  *every* level's dispose — synchronous or resumed — since both paths share
  the same per-level loop body.
  - `route_return!` calls `unwind_for_of!(unwind_from,
    Completion::Return(ret_val.clone()), DisposeThen::ForOfCrossReturn)`.
  - `route_loop_control!` calls `unwind_for_of!(handler_boundary.min(len),
    Completion::Empty, DisposeThen::ForOfCrossLoopControl(target))` (seed
    unchanged from before this change).
  - The inline `Completion::Break` fast path (reached only when a bare,
    unlabeled `break` and its loop are both in a single state with no
    intervening `await`) is converted to build a `LoopControlTarget` and call
    `route_loop_control!`, mirroring the adjacent `Completion::Continue` arm
    exactly instead of calling `unwind_for_of!` directly. Diffed against
    what `route_loop_control!` does that the old inline arm didn't
    (`pending_for_of_unwind = None`, un-entered-`finally` routing via
    `routed_to`, `try_stack.truncate(target.try_depth)`): an unlabeled break
    reaching this arm always targets the innermost loop with no intervening
    `try`/`finally` possible (any such `break` is lowered to a `LoopControl`
    terminator at transform time regardless of `await`, which already calls
    `route_loop_control!` unconditionally — unaffected by this change), so
    `routed_to` is always `None` here and the resulting `try_stack` length is
    identical either way. `pending_for_of_unwind` is reset to `None` by
    `route_loop_control!` where the old arm left it untouched, but that field
    has no other reader in the driver besides the two `clear_at_state`
    checks that clear it again later — confirmed by grepping every
    `pending_for_of_unwind` reference, not assumed — so this is inert for
    every reachable case, verified empirically too: both pre-existing
    oracles (`async-function-for-of-abrupt-completion-unwind.js`,
    `generator-loop-control-closes-for-of-iterators.js`, the latter a
    sync-generator file unaffected by this driver's code at all) stay green
    unchanged. This is a true refactor, not a silent behavior change.
- **Resume-dispatch arms.** The `(DisposeThen, Completion)` match in
  `async_function_resume` gains:
  - `(ForOfCrossReturn, Throw(e))` → `pending_exception = Some(e)`;
    `(ForOfCrossReturn, Return(v))` → `route_return!(v)` (re-entering the
    macro from scratch); `(ForOfCrossReturn, _)` → `unreachable!()`.
  - `(ForOfCrossLoopControl(_), Throw(e))` → `pending_exception = Some(e)`;
    `(ForOfCrossLoopControl(target), _)` → `route_loop_control!(target)`.
  - `(ForOfCrossThrow, Throw(e))` → `pending_exception = Some(e)`;
    `(ForOfCrossThrow, _)` → `unreachable!()`.
  None of the three set `pending_for_of_unwind` on their `Throw` arm (unlike
  `unwind_for_of!`'s own synchronous throw tail, which does, before
  `continue`-ing). This is intentional: the park always leaves the loop
  still on `for_of_stack` with `iteration_env` already taken, not yet
  popped, so when `pending_exception` reaches the top-level throw-routing
  block, `needs_for_of_unwind` is recomputed as `true` from that same
  `for_of_stack`, and `pending_for_of_unwind` gets (re)set correctly there
  if any loop remains open past the handler — the resume arm would be
  racing that recomputation, not helping it.

## Why re-entering the macro from scratch after a resume is correct

Every `DisposeCursor` stores whatever completion it is constructed with
verbatim and returns it unchanged from `finish()` on the no-disposer-error
path, regardless of variant (`Empty`/`Return`/`Throw`/`Break`/`Continue` are
all legal seeds); `current_error` — the `SuppressedError` chain — is seeded
from the completion only when it is already `Throw`. So a seed's identity
survives a park/resume round trip exactly, and a disposer or `IteratorClose`
failure replaces *any* non-`Throw` seed outright (never merges with it). That
means a parked-then-resumed re-entry that extracts the finished value back
out of the resumed `Completion` and re-invokes the *same* macro
(`unwind_for_of!` via `route_return!`/`route_loop_control!`, or the top-level
throw routing via `unwind_async_for_of_loops`) from scratch is provably
equivalent to the original call never having suspended: `try_stack` was
already truncated to the closing loop's `try_depth` before the park, so
re-deriving `unwind_from`/`handler_boundary` from the (unchanged) `try_stack`
reproduces the same boundary; the closing loop is still on `for_of_stack`
with `iteration_env` already `None`, so the fresh macro expansion's first
iteration disposes nothing for it and falls straight through to
`close_for_of_iterator`, then continues unwinding whatever remains exactly
as it would have without the suspension. This is the same idiom
`ScopeCrossReturn`/`ScopeCrossLoopControl`/`ScopeCrossThrow` already use for
scope disposal, now extended one level up to for-of.

## Verification

`test262-extra/async-function-for-of-abrupt-unwind-suspends.js`: a
witness-chain probe (the same technique as
`async-generator-for-of-throw-unwind-suspends.js`) covering five shapes in
one file: an uncaught throw (`unwind_async_for_of_loops`/throw-routing), a
`return` (`route_return!`), an unlabeled `break` with no preceding `await`
(the inline fast path), a labeled `break` after an `await` crossing two
nested loops (`route_loop_control!` via the `LoopControl` terminator), and a
`return` whose inner disposer rejects with an intervening `catch` that must
handle it before the (non-`await using`) outer loop — the guard for
`unwind_for_of!`'s per-level handler-boundary re-check running across a
suspension, since the throw-routing path never reaches that re-check at all
(its `unwind_from` is fixed before the call). All five assert the driver
returns control to its synchronous caller before draining any queued job —
red before this change, green after. The existing sync-dispose-only oracle
(`async-function-for-of-abrupt-completion-unwind.js`) and the per-iteration
environment/return/break regression files stay green unchanged, confirming
no observable ordering or `SuppressedError`-chaining change for the cases
they already covered.

## Known boundaries (not fixed here)

- Everything ADR-2026-09-22-2340 and #761 already cover (the async-generator
  driver) is unaffected; this ADR only touches the async-function driver.
- `ArrayPatternIterOp::Finish` (array-destructuring rest pattern) was
  investigated and found not to be a blocking gap at all: its `ForOfLoopState`
  is always constructed with `iteration_env: None`, so `close_for_of_loop`'s
  call there is already the pure synchronous `IteratorClose` path.

Follow-up to #742 item 4; closes #779.
