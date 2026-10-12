# Typed microtask jobs and await resumption without function objects (#916)

## Change

Builds on #915 (intrinsic-promise reactions).

- The microtask queue stores `scheduler::Microtask`: `Reaction { reaction,
  argument }` and `ResolveThenable { .. }` replace the per-job
  `Box<dyn FnOnce>` plus separately allocated roots `Vec` for the Promise
  machinery. The collector and `run_microtask` read the queued values off the
  job itself. One-off producers (disposal, async generators, modules, `Array.fromAsync`)
  keep the `Closure { roots, job }` form through the unchanged
  `enqueue_microtask` entry point.
- `ReactionTarget::AsyncResume(async_id)` is the `await` continuation. The
  reaction has no handler: the job calls `async_function_resume` with the
  settled value or reason. The `asyncFnFulfill`/`asyncFnReject` native function
  pair per pending await is gone, and a settled promise queues its one job
  through `perform_promise_then_internal` (which also sets
  `[[PromiseIsHandled]]`, as PerformPromiseThen does).
- An `await` of a non-object skips `PromiseResolve` entirely: the only promise
  it could produce is already fulfilled with the value, so the single
  continuation job is queued with the value itself.
- `async_function_resume` leaves the `AsyncFunctionState` in the scheduler map
  instead of remove + re-insert of a full clone, and `async_fn_suspend_at_await`
  moves the driver's `try_stack`/`for_of_stack`/`scope_stack` into the entry in
  place instead of `to_vec()` copies. The resume still clones the entry's
  stacks into driver locals (the collector traces the entry, not the locals),
  so a resume inside nested loops or `try` blocks still allocates them.
- The state records its `realm_id`; `async_function_resume` installs it around
  the resumption. The removed handler functions did this implicitly (a native
  call runs in its function's realm), and the already-settled path never did,
  so this also fixes an await continuation running in the draining realm.
- The three drain loops (`drain_microtasks`, `drain_microtasks_blocking`, the
  nested loop in `await_value`) share `run_microtask`, which owns the root
  frame and the `gc_assert_root_depth` check.

## Measurement

Host load average was 18-25 on 24 cores throughout (other agents), so
absolute numbers are noisy; interleaved runs, CPU seconds (user+sys), median
of 9 with the minimum in brackets.

| benchmark | before (main @ c0391bd8) | after | boa 0.22 |
|---|---|---|---|
| `bench_promises_await_loop` | 0.32 s (0.29) | 0.26 s (0.19) | 0.55 s (0.51) |
| `bench_promises_for_await` | 0.93 s (0.59) | 0.78 s (0.51) | 1.03 s (0.80) |
| `bench_promises_async_call` | 1.05 s (0.93) | 0.94 s (0.68) | 1.13 s (0.80) |

The issue's thresholds (`await_loop <= 1.2 s`, `for_await <= 2.6 s`) were
calibrated on buildbox3, which is several times slower than this host; here
both were already below them before this change, so the ratio to boa and the
before/after delta are the meaningful figures.

The structural gate is `await_on_a_pending_promise_registers_typed_resume_reactions`
in `src/interpreter/tests.rs`: the reactions an `await` leaves on the awaited
promise carry no handler function.

## Not done

- Async-generator request handling (`asyncGenYieldFulfill`/`Reject` for a
  yielded pending promise, the per-request resolve/reject pair) is a separate
  mechanism and is unchanged.
- `await_value` (the blocking inline await used outside the state machine)
  still creates its `awaitFulfill`/`awaitReject` pair for a pending promise.
- `SentValueBindingKind::Variable(String)` is still cloned per await.
- An abrupt `Get(x, "constructor")` on an awaited promise still becomes a
  rejected promise (one extra tick) instead of throwing at the await point;
  unchanged by this work.
