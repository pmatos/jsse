# Async generators: AwaitReturn parks the request, the queue driver no longer drains

Issue #712 (follow-up of #687). The state-machine queue driver settled a
request's promise and then called `drain_microtasks()` before handing the
promise back to a synchronous caller. The drains were load-bearing for one
reason: `async_generator_await_return` costs a microtask tick (its
PerformPromiseThen), but `async_gen_process_queue` popped the request and
started the next one *before* that tick fired. The inline drain hid the
reordering by running the reaction first. It was also a latent bug without the
drain: a pending `return(operand)` let a later `next()`/`throw()` settle first,
and on a few paths (`yield*` inner `return()` completing, `await using` at a
return) the return never settled at all.

## Decisions

- **The queue head is the `draining-queue` marker; no new state variant.**
  `async_gen_enqueue` starts a request only when `!executing && queue_len == 1`
  and `Scheduler::for_each_root` roots the generator plus every queued request.
  Keeping the request at the head across the await therefore *is* the spec's
  `draining-queue` state (§27.6.3.1: the queue is non-empty iff the state is
  `executing` or `draining-queue`) — the same technique ADR-2026-09-21-2015 uses
  for a parked disposal. A `StateMachineExecutionState` variant would touch every
  exhaustive match for no observable change.
- **The reaction owns settle → pop → drain.** `async_gen_await_return` does
  `PromiseResolve(%Promise%, value)` and one `PerformPromiseThen`; both closures
  settle the request first, then `async_gen_drain_queue` pops it and serves the
  rest. Settling before popping matters: `resolve` reads `iterResult.then`, which
  user code can define, and a re-entrant `next()` from there must still find a
  non-empty queue and only be enqueued. The closures are opaque to the GC tracer;
  that is safe only because the head request stays queued.
- **`PromiseResolve` failure is the one synchronous exit.** It rejects the request
  now and returns `Settled`, and the caller finishes as for any synchronous settle
  (spec: CompleteStep(reject) + DrainQueue in the same job).
- **The helper never touches the yield-pending flag.** `async_gen_yield_pending`
  is a process-global that continuation-context callers never reset, so setting it
  from the helper would leak into an unrelated driver invocation. The helper
  returns `Parked | Settled`; driver-context callers translate `Parked` into the
  flag (`async_gen_await_return_in_driver`), continuation-context callers simply
  return because the reaction owns the pop.
- **The drains are deleted.** 34 `drain_microtasks()` calls after settling in
  `async_generator_next_state_machine_impl` and the return/throw wrappers are
  gone. Request order now comes from the queue, not from running jobs inside
  `it.next()`/`return()`/`throw()`.

## Known boundaries (unchanged, not fixed here)

- `reject_with_type_error` and the eval.rs parameter-binding drain remain (#714).
- `for await` step errors and the inline-yield fallback still block on
  `await_value` (#707, #710 under #687), so those paths still run a job inside
  the call.
- The `Executing` arms of the next/return/throw entry points reject with a
  `TypeError` where the spec enqueues; they are unreachable because
  `async_gen_enqueue` never starts a request on an executing generator.
- ADR-2026-09-21-2300's line saying `async_generator_await_return` drains is
  historical and stays as written.
