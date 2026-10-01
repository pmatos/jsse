# Plan: issue #780 — async generators: no-`.throw()`-method `yield*` arm needs `AsyncIteratorClose` before `TypeError`

## 1. Problem restated

In `async_generator_next_state_machine_impl`'s delegation prelude
(`src/interpreter/eval/generator_runtime.rs:3603-3636`), when a `.throw()`
request is in flight during `yield* iterable` delegation and the delegate
has no `.throw` method (`self.iterator_throw(&iterator, &exc)` returns
`Ok(None)`), the current code (lines 3619-3627) calls the shared synchronous
`iterator_close` helper, discards its result with `let _ = ...`, and then
unconditionally rejects the generator's result promise directly with a
`TypeError`. This is wrong in three compounding ways: (1) `iterator_close`
never calls `Await` on the delegate's `.return()` result, so a delegate
whose `.return()` is itself async is never actually awaited before the
`TypeError` fires; (2) any error from `.return()` — a synchronous throw, or
an asynchronous rejection — is silently dropped (`let _ =`) instead of
taking priority over the planned `TypeError`, as `AsyncIteratorClose`
(`sec-asynciteratorclose`) step 6 requires; and (3) the `TypeError` is delivered by rejecting the
`next()`/`throw()` request's promise directly, bypassing the generator
body's own `try`/`catch`/`finally`, instead of being thrown into the body
the way every other delegation abrupt-exit arm in this function now does
(since #781).

## 2. Spec basis

- `spec/spec.html:24279-24316` — `sec-generator-function-definitions-runtime-semantics-evaluation`,
  `YieldExpression : yield * AssignmentExpression`. Counting the actual
  `emu-alg` nesting: the outer `Repeat` is step 8; `received is a throw
  completion` is step 8.b; `throw is not undefined` is step 8.b.ii
  (already correct — delegates through `yield_star_suspend_on_inner_result`);
  the arm under fix is the `Else` at step 8.b.iii:
  - 8.b.iii.2: `Let closeCompletion be NormalCompletion(~empty~)`
  - 8.b.iii.3: `If generatorKind is ~async~, perform ? AsyncIteratorClose(iteratorRecord, closeCompletion)`
  - 8.b.iii.6: `Throw a TypeError exception` — reached only if step
    8.b.iii.3's `?` did not already abort with a different abrupt
    completion.
  (The issue body's own cited range, `spec.html:24310-24316`, is this
  same `Else` branch — step 8.b.iii's six substeps.)
- `spec/spec.html:7220-7245` — `sec-asynciteratorclose`, `AsyncIteratorClose
  ( iteratorRecord, completion )`. Its own steps:
  1. Assert the iterator is an Object; 2. `iterator` := `iteratorRecord.[[Iterator]]`.
  3. `innerResult` := `Completion(GetMethod(iterator, "return"))` — may
     itself throw (abrupt).
  4. If `innerResult` is normal: (a) `return` := its value; (b) if
     `return` is `undefined`, return `? completion` unchanged (here:
     normal/empty — falls through to step 8.b.iii.6's `TypeError`);
     (c) otherwise `Call(return, iterator)` with **no arguments**; (d) if
     that call completed normally, set `innerResult` to
     `Completion(Await(innerResult.[[Value]]))`.
  5. `If completion is a throw completion, return ? completion` — **does
     not apply at this call site**, because the `closeCompletion` passed
     in is always `NormalCompletion(~empty~)` (step 8.b.iii.2), never a
     throw completion. So, uniquely here, an abrupt `innerResult` (from
     the `GetMethod`, the `Call`, or the `Await`) always overrides the
     step-8.b.iii.6 `TypeError` that would otherwise follow — this is the
     "which error wins" subtlety the issue flags.
  6. `If innerResult is a throw completion, return ? innerResult` — this
     is what must now take priority over the `TypeError`, instead of
     being dropped by today's `let _ = self.iterator_close(...)`.
  7. `If innerResult.[[Value]] is not an Object, throw a TypeError
     exception` — a *second*, distinct `TypeError` (checked on the
     post-`Await` value, not the pre-`Await` `Call` result), also
     overriding step 8.b.iii.6's.
  8. Otherwise `return ? completion` (normal) — falls through to step
     8.b.iii.6's `TypeError` ("the iterator does not provide a throw
     method").
- `spec/spec.html:6278-6295` — `sec-getmethod`, `GetMethod ( V, P )`: `? GetV(V,
  P)`; `undefined`/`null` → return `undefined` (no method); otherwise if
  `IsCallable` is `false`, **throw a TypeError** (this is the case the
  sibling `iterator_return`/`iterator_throw` helpers in
  `src/interpreter/builtins/iterators.rs:4987-5049` get wrong today — they
  treat any non-object property value as "no method" instead of
  distinguishing "`undefined`/`null`" from "non-callable" — not something
  this plan fixes in those shared helpers; see §7).
- Delivery-into-body precedent: commit #781 (`fix(generators): deliver
  yield* abrupt exits into the body, fix return Await order`) established
  that every delegation abrupt-exit arm in `async_generator_next_state_machine_impl`
  must land its completion where the ordinary `check_abrupt_on_resume` path
  (or, for an already-suspended continuation, `deliver_yield_star_completion`)
  routes it through the body's own `try`/`catch`/`finally` and outer for-of
  unwind — never by rejecting the request promise directly. This arm is the
  one case #781 explicitly left out (per #742 item 3 and the #780 issue
  body), and this plan brings it in line with that precedent. See
  `docs/adr/2026-09-22-2326-async-generator-return-operand-and-parked-unwind.md`
  and `docs/adr/2026-09-22-2340-async-generator-for-of-unwind-suspends.md`.

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — rewrite the `Ok(None)` arm
  at lines 3619-3627 inside `async_generator_next_state_machine_impl`
  (3497-5577). Add one new private helper (and, if the await path needs a
  dedicated resume continuation, a second small `fn ..._resume` beside it,
  following the shape of `yield_star_suspend_on_inner_result` /
  `yield_star_await_inner_result_resume` at lines 3021-3062 and the
  existing no-`.return()`-method arm at lines 3558-3589). No new
  `ObjectKind`/`IteratorState` variant, no new side table — this reuses
  `deliver_yield_star_completion` (2982-3013) and `await_then`
  (`src/interpreter/dispose.rs:377-388`) exactly as the sibling arm does.
- `docs/adr/2026-09-22-2340-async-generator-for-of-unwind-suspends.md` and/or
  `docs/adr/2026-09-22-2326-async-generator-return-operand-and-parked-unwind.md`
  — append a short bullet (matching the style #781 used) noting that the
  no-`.throw()`-method arm now performs `AsyncIteratorClose` (awaiting an
  async `.return()`, prioritizing its rejection or non-object result over
  the protocol-violation `TypeError`) and delivers into the body, closing
  out the last item named by #742/#780. No new ADR file — this is an
  application of the already-decided pattern, not a new decision.
- `CONTEXT.md` — no new vocabulary; `AsyncIteratorClose` and `yield*`
  delegation are already-established terms there (per #781's precedent of
  only touching `CONTEXT.md` when terminology actually changes — check
  before assuming an edit is needed; likely a no-op here).
- `test262-extra/` — new test files (see §5).

## 4. TDD slices

Each slice is red (write the test, confirm it fails against current
`main`-derived behavior) then green (land the minimal code to pass it),
following the existing `iterator_close`/`yield_star_suspend_on_inner_result`/
`deliver_yield_star_completion` machinery already in the file (see §2's
`GetMethod` note for why `iterator_return`/`iterator_throw`'s lookup shape
is *not* the one to copy). Build with `cargo build --release` between
slices (cap parallelism explicitly for the host's shared build budget,
e.g. `cargo build --release -j<N>` rather than the tool default); run the
targeted test262 directories (§5) plus the new `test262-extra` file after
each slice, not the full suite, to stay fast in-loop — run the full
`test262`/`test262-extra` sweep once at the end.

1. **Delegate has no `return` method at all (undefined/null): TypeError
   fires immediately, synchronously, no suspension.**
   This is close to today's already-passing behavior (modulo the
   direct-reject-vs-deliver-into-body bug) and the cheapest slice to land
   first because it requires no new `await_then` continuation — just
   routing the already-known-synchronous `TypeError` through
   `stored_pending_exception = Some(type_err); pending_binding = None;
   break 'delegation;` (mirroring the `Err(e)` arms immediately above/below
   it at lines 3590-3599 and 3628-3635) instead of calling
   `reject_async_generator_request` directly.
   - Test: new `test262-extra/async-generator-yield-star-no-throw-method-type-error-into-body.js`
     — a `try { yield* delegateWithNoThrow; } catch (e) { ... } finally { ...
     }` body; assert the `TypeError` is caught by the body's own `catch`
     (and the generator can continue/complete normally afterward), not
     just observed as a rejected `.throw()` promise. Covers the delivery
     mechanism, independent of the `AsyncIteratorClose` await question.
   - Also re-run `test262/test/language/expressions/async-generator/yield-star-getiter-async-throw-method-is-null.js`
     (and the `statements/` counterpart if one exists) to confirm no
     regression — that test's delegate's `return` getter itself returns
     `undefined`, so it already exercises "GetMethod found an accessor
     that yields no method", just not from inside a `try`/`catch`.

2. **`GetMethod(iterator, "return")` itself throws (e.g. a throwing
   getter, or a non-callable `return` property): that error overrides the
   `TypeError`, synchronously, no suspension.**
   - Test: extend the same new test262-extra file (or a sibling
     `async-generator-yield-star-no-throw-method-return-getter-throws.js`)
     with two delegates: one whose `return` property is a throwing
     getter, and one whose `return` property is a plain non-callable
     value (e.g. `return: 42`, no getter) — `GetMethod` step 3 requires
     the latter to throw `TypeError` too, via `IsCallable` being `false`,
     not to be treated as "no method". Assert the generator body's
     `catch` observes the getter's own error (first case) or a
     `TypeError` (second case), not the "the iterator does not provide a
     throw method" `TypeError`.
   - Code: do **not** reuse `iterator_return`'s/`iterator_throw`'s lookup
     shape (`Completion::Normal(v) if v.is_object() => Some(v), Normal(_)
     => None`, `iterators.rs:4992-4998`/`5025-5030`) — that filter treats
     *any* non-object property value, including a non-callable one, as
     "no method", which is not `GetMethod`'s step-3 behavior. Instead
     mirror `iterator_close`'s own lookup shape at
     `iterators.rs:5063-5074` (`get_object_property` → `Completion::Throw`
     propagates; `undefined`/`null` → no method; `!self.is_callable(&v)`
     → `TypeError`) but, unlike `iterator_close`, **propagate** each of
     these outcomes instead of discarding it: a `Completion::Throw(e)` or
     an `is_callable`-false `TypeError` both route to the same
     `stored_pending_exception`/`break 'delegation` path as slice 1,
     carrying that error instead of `type_err`. Exactly one property read
     of `"return"` must happen (`yield-star-getiter-async-throw-method-is-null.js`
     asserts `returnGets === 1` for the analogous `throw` lookup; the same
     invariant applies here for `"return"`).

3. **`.return()` exists, is called, and the call itself throws
   synchronously (before any `Await`): that error overrides the
   `TypeError`, synchronously, no suspension.**
   - Test: delegate's `return(v)` is a plain function that throws.
     Same assertion shape as slice 2.
   - Code: `call_function(&return_fn, iterator, &[])` — `AsyncIteratorClose`
     step 4.c calls `Call(return, iterator)` with *no* arguments (unlike
     `iterator_return`'s delegated-return step, which passes the received
     value). `Completion::Throw(e)` routes the same way as slice 2.

4. **`.return()` call succeeds normally and the result is already a
   plain (non-thenable) object: `Await` still costs a microtask —
   `Await` of a non-thenable value is not free, it always suspends
   through at least one microtask turn (`promise_resolve_value` wraps it
   and the continuation runs from the microtask queue, same as every
   other `await_then` call in this file) — confirm the `TypeError` ("no
   throw method") fires only after that turn, and only because the
   awaited value is an object.**
   This is the first slice that needs `await_then`: call
   `self.await_then(&call_result, move |interp, outcome| { ... })`
   directly on the `Call` result (matching
   `yield_star_suspend_on_inner_result`'s "await the call result
   directly, no `is_promise` pre-check" pattern at lines 3045-3056 — this
   also correctly fixes the pre-existing `iterator_return`/`iterator_throw`
   quirk of checking `is_object()` *before* `Await` instead of after,
   without touching those shared helpers), set
   `self.scheduler.set_async_gen_yield_pending(true)`, and `return
   Completion::Normal(promise)` — mirroring the no-`.return()`-method arm
   at lines 3567-3589 exactly, but keyed on `Call`+`Await` success/failure
   instead of awaiting the received value directly. Clear
   `stored_pending_exception`/`pending_return` before suspending (mirror
   `yield_star_suspend_on_inner_result`'s own clearing at lines 3031-3040),
   so the already-taken `exc` doesn't linger as stale object state across
   the suspension. In the resume closure: if `outcome` is `Ok(v)` and
   `v.is_object()`, build `Completion::Throw(type_err)` (the original "no
   throw method" `TypeError`, constructed once up front and moved into
   the closure); call `deliver_yield_star_completion(o.id,
   state_machine.clone(), func_env.clone(), is_strict, try_stack.clone(),
   &deleg_info, completion, (&promise_c, &resolve_c, &reject_c))`, exactly
   like the sibling arm.
   - Test: delegate's `return(v)` returns a plain object synchronously
     (`{done: true, value: v}`, no `Promise.resolve` wrapper) in a body
     with a `try`/`finally` (no `catch`) around the `yield*`. Queue a
     marker microtask (`Promise.resolve().then(() => log.push('marker'))`)
     right after issuing the `.throw()` call and assert, via the log
     array, that `'marker'` fires *before* the `finally` runs and before
     the `TypeError` is observed — this is what would fail if the
     implementation skipped `await_then` for a "same-tick-looking"
     non-thenable result. New
     `test262-extra/async-generator-yield-star-no-throw-method-return-sync-object.js`.

5. **`.return()`'s result is a thenable that resolves asynchronously
   (genuine cross-tick suspension): confirm the generator parks at the
   `Await`, that intervening microtasks run before the `TypeError`
   settles, and that `.return()` was called exactly once.**
   - Test: `test262-extra/async-generator-yield-star-no-throw-method-return-is-awaited.js`,
     modeled on `async-generator-yield-star-return-completion-await-ordering.js`'s
     `drainTicks`/log-array idiom — delegate's `return(v)` returns a
     `Promise` that resolves after N queued microtasks; assert the ordering
     log shows the queued ticks interleaving before the `TypeError` lands,
     and that `calls.return === 1`.
   - No new production code expected beyond slice 4's `await_then` wiring
     — this slice is the regression lock for "the `Await` actually
     suspends instead of being skipped", i.e. the issue's core complaint.

6. **`.return()`'s call succeeds but the (post-`Await`) result is not an
   Object: a `TypeError` still fires (`AsyncIteratorClose` step 7), but
   only after the `.return()` call's `Await` settled — not the
   step-8.b.iii.6 `TypeError` that would fire with no `.return()` call at
   all.**
   - Test: delegate's `return(v)` returns `Promise.resolve(42)` (or a bare
     non-object). Both `TypeError`s share the `TypeError` constructor and
     are indistinguishable at the spec level, so do **not** assert on
     `.message` text (not a test262 pattern, and not spec-mandated
     wording) — instead assert `err.constructor === TypeError`, that
     `calls.return === 1` (the delegate's `.return()` was called exactly
     once), and — reusing the slice-4 marker-microtask idiom — that a
     queued marker fires before the rejection settles, proving the
     `Await` actually ran rather than the result being rejected
     synchronously off the `Call`.
     `test262-extra/async-generator-yield-star-no-throw-method-return-resolves-non-object.js`.

7. **`.return()`'s call succeeds but the `Await` rejects: the rejection
   reason overrides the "no throw method" `TypeError`.**
   - Test: delegate's `return(v)` returns `Promise.reject(err)`. Assert
     the body's `catch` sees `err` itself, not a `TypeError`.
     `test262-extra/async-generator-yield-star-no-throw-method-return-rejects.js`.
   - This is the exact scenario named in the issue body ("a rejection
     from that `.return()` call ... is silently dropped").

8. **Enclosing `for-of`/`finally` unwind runs correctly around whichever
   final completion (steps 1-7) is delivered**, exercised by reusing the
   existing `unwind_generator_for_of_loops`/`route_generator_exception`
   machinery via `deliver_yield_star_completion` → `async_gen_reenter` →
   ordinary `check_abrupt_on_resume`. No new test file required if slices
   1-7's bodies already wrap the `yield*` in `try`/`finally`; if the
   implementation stage finds a for-of-specific gap, add one targeted
   `test262-extra` case mirroring
   `async-generator-yield-star-abrupt-exit-closes-outer-for-of.js`.

## 5. Test surface

- Targeted test262 run (should stay green, and exercises the already-passing
  edge of this code path):
  `uv run python scripts/run-test262.py test262/test/language/statements/async-generator/`
  and
  `uv run python scripts/run-test262.py test262/test/language/expressions/async-generator/`
  — in particular `yield-star-getiter-async-throw-method-is-null.js` and
  its `named-yield-star-*` sibling under `expressions/async-generator/`
  cover "GetMethod finds an accessor that returns no method", but neither
  wraps the `yield*` in a `try`/`catch`, awaits an async `.return()`, nor
  checks error priority — none of slices 2-8's behavior is covered by
  test262 today. That gap is exactly why this issue calls for
  `test262-extra/` coverage.
- Also run `uv run python scripts/run-test262.py test262/test/built-ins/AsyncGeneratorFunction/`
  and the broader `built-ins/AsyncGeneratorPrototype/` directory as a
  sanity sweep for the shared `async_generator_next_state_machine_impl`
  entry points, even though they don't target this exact arm.
- New `test262-extra/async-generator-yield-star-no-throw-method-*.js`
  files from §4 (run via `uv run python scripts/run-test262.py
  test262-extra/` per the project's documented no-dedicated-runner
  convention — pass the directory straight to `run-test262.py`).
- Full gate before considering the slice series done: `cargo test
  --release` (engine unit/integration tests, unaffected by this change but
  must stay green) and the full `uv run python scripts/run-test262.py`
  sweep (not just the targeted directories) to catch any cross-cutting
  regression in shared delegation code before relying on CI's own run.
- `./scripts/lint.sh` after the Rust changes.

## 6. Regression risk

- **Shared helpers are read-only.** `iterator_close`, `iterator_return`,
  and `iterator_throw` (`src/interpreter/builtins/iterators.rs`) are not
  modified — the issue explicitly calls for a dedicated async-aware
  continuation instead, and those helpers are shared by every sync
  iteration call site in the codebase (for-of loops, spread, destructuring,
  `Array.from`, etc.), so touching them risks a much wider blast radius
  than this arm.
- **New code is additive within one `match` arm** of
  `async_generator_next_state_machine_impl`, which is already a very large
  function; the new helper(s) should be free functions/methods beside it
  (like `yield_star_suspend_on_inner_result`), not more inline nesting, to
  keep the existing arm's diff small and reviewable.
- **The sync generator's analogous arm is not affected and is not a
  parallel bug.** `generator_next_state_machine_impl`'s own "no `.throw()`
  method" arm (`generator_runtime.rs:2600-2625`) already calls
  `iterator_close_result` (not `iterator_close`), which *does* propagate a
  `.return()` error (`Err(e) => return
  self.generator_throw_state_machine(this, e)`) ahead of forming the "no
  throw method" `TypeError` — i.e. it already implements the sync
  equivalent of `IteratorClose`'s error-priority rule correctly; the only
  thing it lacks (because sync generators never `Await`) is the suspension
  this plan adds for the async case. Confirmed by reading the code before
  writing this plan — not a second instance of the bug, and not a
  follow-up item.
- **GC rooting**: the awaited `.return()` call result must be rooted for
  the duration of the suspension. `await_then` already roots its `value`
  argument via `with_gc_root_scope`/`gc_root_value` (`dispose.rs:382-387`)
  before scheduling the resume, which is the same mechanism
  `async_generator_return_state_machine_with_promise` (6160-6269) and
  `yield_star_suspend_on_inner_result` (3021-3057) rely on — as long as the
  new code calls `await_then` the same way (pass the `Call` result
  directly, don't pre-root or stash it elsewhere), no new rooting work is
  needed. Getting this wrong (e.g. holding the call result across a
  `gc_safepoint()` without going through `await_then`) would reintroduce a
  use-after-GC hazard class the project has hit before (see the
  `gc-root-scope-guard`/`gc-root-scope-guard-eval` entries in the
  architecture backlog).
- **`StateMachineExecutionState`/`IteratorState` shape is unchanged** — no
  new `ObjectKind` variant, no new side table (unlike some other items in
  #742, e.g. the for-of loop-control one), so `gc::trace_object_fields`'s
  exhaustive match needs no update and the GC walker risk here is limited
  to the point above.
- **Could move `test262-pass.txt`-tracked tests**: any test262 case
  touching `yield*` delegation with a throw in flight and a delegate
  lacking `.throw` is the blast radius; the targeted directories in §5
  are the ones to watch. Given how narrow and currently-mishandled this
  arm is, the expected direction is tests moving from fail→pass, not the
  reverse — but the full sweep in §5 is there to catch an unexpected
  regression before it reaches CI.
- **Behavior-visible change**: as `docs/adr/2026-09-22-2340-...md` already
  notes for the sibling arms #781 fixed, routing the completion through
  `deliver_yield_star_completion` instead of rejecting the request
  promise directly is an observable semantics change (a `try`/`catch`
  around the `yield*` now catches what used to bypass it straight to the
  rejected promise). This is required by the spec (the `TypeError` is
  thrown *by the `YieldExpression` evaluation*, i.e. inside the body, per
  step 8.b.iii.6), not optional, and is exactly what #780 asks for.
- **Tree-walker hot path / bytecode fast path**: generators do not
  compile through the bytecode VM (`bytecode_enabled` is off by default
  and the generator state machine is a dedicated lowering, not part of
  `eval_expr`/`exec_statement`'s normal dispatch), so this change has no
  interaction with the bytecode fast path. `property.rs`'s MOP operations
  are exercised only incidentally (via `get_object_property`/`call_function`
  for `GetMethod`/`Call`), with no new property-access pattern introduced.
- **Node-compat library harnesses**: none of the wired libraries
  (`decimal.js`, `acorn`, `zod`, `moment`, etc.) are known to exercise
  async-generator `yield*` delegation against a delegate missing `.throw`
  while a `.throw()` request is in flight; no interaction expected, but
  the full `test262`/`cargo test --release` gate in §5 is the backstop.

## 7. Out of scope

- Fixing `iterator_return`/`iterator_throw`'s pre-existing `GetMethod`
  imprecision (treating a non-callable-but-object `return`/`throw`
  property as "no method" instead of throwing `TypeError`, and checking
  `is_object()` on the pre-`Await` `Call` result rather than the
  post-`Await` one) — real, but shared by other call sites and not named
  by #780; a separate issue if it turns out to be user-observable.
  (Note: slice 4 above sidesteps this for the *new* code by awaiting the
  raw `Call` result directly, as `AsyncIteratorClose` requires, rather
  than reusing `iterator_return`'s object-check-before-`Await` shape — but
  it does not retrofit that fix onto the shared helpers themselves.)
- The four other remaining items from #742 explicitly *not* this one:
  `align_generator_for_of_stack`/`route_generator_loop_control`'s blocking
  dispose, the inline-yield-replay disposal path, and async functions'
  `close_for_of_loop` blocking dispose. None of those are touched here.
- No refactor of `async_generator_next_state_machine_impl`'s overall
  structure, no extraction of the delegation prelude into its own
  function, no renaming — this PR's diff is the one arm plus its new
  helper(s).
- No change to `reject_async_generator_request` itself (still used by
  other, unrelated call sites) and no change to `deliver_yield_star_completion`'s
  own signature or behavior.
- Rolling `test262-pass.txt` forward — left to `main`, per repo convention.
