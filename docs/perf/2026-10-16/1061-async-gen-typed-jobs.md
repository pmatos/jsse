# #1061: async-generator requests and awaits without function objects

Builds on #916 (typed `AsyncResume` reactions and `PromiseResolvers::Internal` for async functions), which left
the async-generator request pair and per-await handlers behind.

## Change

1. Requests. `AsyncGenRequest { kind, value, promise }`: the request promise is built from `%Promise%`, its
   resolving functions were never observable, so it is settled through `PromiseResolvers::Internal`
   (`async_gen_resolve_request` / `async_gen_reject_request`; `claim_internal_resolution` keeps
   `[[AlreadyResolved]]`). The `(promise, resolve_fn, reject_fn)` triple threaded through `generator_runtime.rs` and
   `GeneratorDisposal` collapsed to the promise, and `create_resolving_functions` (two native functions, two
   `pin_native_root`s and a `resolving_functions` back-edge per request) is gone from `.next()/.return()/.throw()`.
2. Awaits. `ReactionTarget::AsyncGen(gen_id, AsyncGenStep::Await)` has no handler. The reaction job takes the
   promise of the request at the front of `async_gen_queue(gen_id)` and calls `async_gen_await_resume`.
   `async_gen_suspend_at_await` follows `async_fn_suspend_at_await`: `PromiseResolve` only for objects, a
   non-object queues its one reaction job with the value itself, and the `try_stack` is moved into the saved
   state instead of `to_vec()`.
3. Same mechanism, the rest of the family: a `yield` of a pending promise (`AsyncGenStep::Yield`), the
   AsyncGeneratorAwaitReturn handlers (`AsyncGenStep::AwaitReturn`) and the `yield <settled value>` continuation
   (`Microtask::AsyncGenYield`, replacing a `Closure { roots Vec, Box }`) all complete the front request through
   `async_gen_reaction`.

Why the front request is the right one: a suspended step belongs to the request at the head of the queue, which
is popped only after that request settles (the driver leaves it alone while `async_gen_yield_pending` is set), and
`Scheduler::for_each_root` roots a generator only while its queue is non-empty, so the typed target carrying a bare
generator id is as safe as the closures it replaces; an empty queue makes the job a no-op.

Tick counts are unchanged: every await, yield-await and return-await is still exactly one reaction job from the same
`PerformPromiseThen` on the same promise (`perform_promise_then_internal` sets `[[PromiseIsHandled]]`).
`perform_await_then` had no remaining caller and was removed.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, empty script 7,486,186 subtracted for the net figures. base =
`origin/main` at `029b06a7` (binary sha256 `9fdac928a458...`), new = the same tree plus this change (`f92330856e30...`).
The isolating scripts are this PR's reading of the issue's variants (6,000 requests / 1,500 iterations x 4 awaits);
absolute numbers differ from the issue's, the ratios are what matter.

| workload | base Ir | new Ir | net delta | criterion |
|---|---:|---:|---:|---|
| `repro.js` pipeline (net) | 159,698,265 | 108,462,567 | -32.1% | >= 25% |
| `req.js` 6,000 `await it.next()` (net) | 149,701,779 | 114,152,025 | -23.7% | >= 15% |
| `await.js` 4 awaits per item (net) | 115,122,970 | 54,923,490 | -52.3% | >= 35% |
| `ctl-await-gen.js`, Ir per await | 15,242 | 7,078 | -53.6% | <= 10,000 |
| `ctl-await-fn.js`, Ir per await (async function control) | 5,400 | 5,405 | +0.1% | within +-2% |
| async-fs FULL | 11,047,473,938 | 9,909,589,175 | -10.3% | >= 8% |
| async-fs setup-only | 5,126,308,390 | 5,118,198,581 | -0.2% | unchanged |

async-fs scored iteration (FULL minus setup-only): 5,921,165,548 -> 4,791,390,594 Ir, -19.1%. callgrind
(`jsse-proflto`-shape build, call edges into `create_function_with_name`): FULL 332,982 -> 960 calls, setup-only
2,562 -> 960, so the scored iteration goes from 330,420 calls (195,502 from `create_resolving_functions`, 136,520 from
`async_gen_suspend_at_await`; the sum minus the setup-only run's 2,562) to zero. Timed iteration of the async-fs
driver (`taskset` one core, alternating, shipped build, load 1-2): base 1711/1706/1713/1703/1700 ms (median 1706), new
1359/1346/1340/1332/1330 ms (median 1340), -21.5%.

### Controls (Ir, same binaries; "no Ir regression > 1%")

| control | base | new | delta |
|---|---:|---:|---:|
| `bench_promises_for_await` | 3,943,864,811 | 3,365,503,127 | -14.7% |
| `bench_promises_async_fs` | 11,038,775,382 | 9,908,727,948 | -10.2% |
| `bench_promises_await_loop` | 2,300,284,044 | 2,301,071,941 | +0.03% |
| `bench_promises_async_call` | 5,348,069,069 | 5,353,305,438 | +0.10% |
| `bench_promises_then_chain` | 3,384,936,307 | 3,388,177,838 | +0.10% |
| `bench_promises_new_promise` | 6,269,657,996 | 6,270,993,895 | +0.02% |
| `bench_promises_queue_deep` | 3,967,232,550 | 3,973,959,968 | +0.17% |
| `bench_generators_simple_yield` | 8,237,694,108 | 8,237,692,882 | 0.00% |
| `bench_generators_manual_next` | 5,399,131,907 | 5,405,191,082 | +0.11% |
| `bench_generators_pipeline` | 7,976,737,191 | 7,974,374,805 | -0.03% |
| `bench_generators_lazy_collections` | 8,807,789,087 | 8,816,118,574 | +0.09% |
| sync-fs (JetStream) | 12,903,594,180 | 12,908,624,378 | +0.04% |
| lazy-collections (JetStream) | 8,872,618,897 | 8,872,447,342 | 0.00% |

The +0.1% rows are the randomised identifier interner's run-to-run spread, not a code-path change (none of these
workloads touches an async generator).

## Correctness gates

Full test262 default and `--no-bytecode`: 99,911/99,911 each. test262-extra 1,416/1,416 normal and under
`JSSE_GC_STRESS=7`, on the plain build and on `release-checked` (root-stack asserts), in both modes. test262
AsyncGeneratorPrototype, async-generator statements/expressions, await, for-await-of and yield: 4,496/4,496 under
`JSSE_GC_STRESS=1` and `=7` on `release-checked`. `tests/` custom runner 19/19, `cargo test --lib` 1,103 passed,
`./scripts/lint.sh` clean.

New test262-extra tests: `async-generator-await-pending-then-settled-queue-order.js` (one generator awaiting a
pending promise, a settled promise, a non-object and a thenable, yielding pending and settled promises, with queued
`next`/`return`/`throw`, against an independent tick counter; expected log taken from node and identical on base),
`async-generator-request-result-thenable-assimilation.js` (`Object.prototype.then` observed, assimilating and
throwing against a request result) and `async-generator-await-continuation-runs-in-function-realm.js`.

## Not done

- `yield*` delegation and disposal await through `await_then` closures (unchanged, out of scope).
- The trailing `enqueue_microtask` closure in `async_gen_await_resume` (queue still non-empty after a request
  completes) is unchanged.
- Found while writing the matrix test, present before this change: `yield Promise.reject(x)` inside `try` is not
  catchable in the generator, now #1082 (the rejected-operand path retires the generator and rejects the request instead of
  throwing at the `yield`; node catches it).
