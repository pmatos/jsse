# Async generators: `.return()` operand, delegated-return disposal and parked unwinds

Issue #716, following ADR-2026-09-21-2015 and ADR-2026-09-21-2300. Six
blocking or missing-suspension paths remained in the async-generator driver.
Two of them were not tick-only: a rejected `.return(v)` operand skipped the
`catch` around the yield, and a `.return()` parked in `yield*` never ran the
generator's disposers.

## Decisions

- **`.return(v)` at a yield Awaits `v` before the generator sees a return**
  (`sec-asyncgeneratorunwrapyieldresumption`). `async_generator_return_state_machine_with_promise`
  keeps its synchronous broken-`constructor` pre-check, then parks on
  `await_then(v)` with the request at the queue head and the generator still
  suspended at its yield. The continuation re-enters the driver
  (`async_gen_reenter`) with the awaited value as `pending_return`, or the
  rejection reason as `pending_exception`: a rejected operand is a throw
  completion at the yield, so a surrounding `catch` handles it and a `yield*`
  delegate receives `throw` rather than `return` (both the direct path and the
  return queued behind a delegated yield). `pending_return` is therefore always
  an already-awaited value: the driver's unwind settles with
  `GeneratorDisposeThen::Settle` and resolves `{ value, done: true }` directly
  instead of awaiting again. `return expr;` through a `try/finally` reaches the
  same function, so its single `Await(exprValue)` now precedes the `finally`.
- **A `yield*` that ends with a return completion disposes the generator's
  resources.** The three arms that finish a delegated return (the inner
  `return` reporting `done`, and the no-`return`-method arms in the driver and
  in `yield_star_return_after_unwrap`) call `async_gen_dispose` with
  `GeneratorDisposeThen::ReturnAwait` (`yield_star_complete_with_return`
  in job context) instead of marking the generator completed. `ReturnAwait`
  now means only this: the value has not been Awaited by the unwinding, so it
  is awaited after disposal, as for a resource-free generator.
- **Frame disposal parks with the in-flight completion.** The frame-leave loop
  seeds its `DisposeCursor` with `Completion::Throw`/`Completion::Return` and
  parks at every Await; the cursor owns the in-flight completion (it is rooted
  through `for_each_value`) and the saved state carries neither
  `pending_exception` nor `pending_return`. `async_gen_reenter` restores the
  finished completion — a throw (a disposer's error replaces the return) or a
  return — and the driver re-routes it against the already-truncated try stack.
  Routing selects a handler by its `entered_*` flags, which are set only when a
  handler state runs, so re-routing finds the same handler: a `finally`
  selected for a return runs once.
- **A `for (await using x of …)` head parks too.** The iteration environment's
  disposal is a `DisposeCursor` stepped in the `ForOfHead` arm; the loop stays
  on the for-of stack with `iteration_env` already taken, so a throwing
  disposer is raised as an exception and routing's for-of unwind closes the
  iterator once. Async generators now lower a for-of whose head is
  `await using` (`stmt_has_suspension`, `stmt_contains_await_using_head`), as
  async functions already did; before, a loop whose body neither yielded nor
  awaited ran natively and disposed inline.

## Known boundaries (not fixed here)

- Inline yield replay (`is_inline_replay`) and the `for-of` unwind
  (`dispose_scopes_inside_for_of`, `close_for_of_loop`'s iteration environment)
  still dispose through the blocking driver. Unwinding a `for-of` must become a
  resumable operation across its five callers first.
- Delegated `yield*` abrupt exits do not run enclosing `finally` blocks or
  close outer `for-of` loops, and a rejected inner result rejects the request
  instead of throwing into the body's `try`/`catch`. The rejected-inner-result
  and `IteratorValue`-throw arms of the delegated-return step also still skip
  DisposeResources.
- `AsyncGeneratorAwaitReturn` (`.return(v)` on a suspended-start or completed
  generator) still drains inline and does not keep the request queue blocked
  (#712); the `yield` operand, `yield*` delegated calls and `for await` steps
  still use a blocking `await_value` (#687).
- A `for (await using …)` nested in a container with no `await`/`yield` of its
  own (`try`, `if`, …) is not lowered, in async generators and async functions
  alike, and still disposes inline.
- The "throwing disposer replaces an in-flight `return` and does not re-enter a
  `finally` already selected" defect ADR-2026-09-21-2015 named was not
  reproducible: every probed shape matched the reference engine, and the
  restored behaviour is pinned in
  `test262-extra/async-generator-await-using-block-exit-inflight-return-suspends.js`.
