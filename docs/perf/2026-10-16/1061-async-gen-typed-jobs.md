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
   `async_gen_suspend_at_await` and `async_fn_suspend_at_await` share `await_promise_resolve` /
   `await_then_internal`: `PromiseResolve` only for objects, a non-object queues its one reaction job with the
   value itself, and the `try_stack` is moved into the saved
   state instead of `to_vec()`.
3. Same mechanism, the rest of the family: a `yield` of a pending promise (`AsyncGenStep::Yield`), the
   AsyncGeneratorAwaitReturn handlers (`AsyncGenStep::AwaitReturn`) and the `yield <settled value>` continuation
   (a handler-less `AsyncGenStep::Yield` reaction job, replacing a `Closure { roots Vec, Box }`) all
   complete the front request through `async_gen_reaction`, which installs the generator's realm around the settlement.

Why the front request is the right one: a suspended step belongs to the request at the head of the queue, which
is popped only after that request settles (the driver leaves it alone while `async_gen_yield_pending` is set), and
`JobScheduler::for_each_root` roots a generator only while its queue is non-empty, so the typed target carrying a bare
generator id is as safe as the closures it replaces; an empty queue is a `debug_assert` failure (a no-op in release builds).

Tick counts are unchanged: every await, yield-await and return-await is still exactly one reaction job from the same
`PerformPromiseThen` on the same promise (`perform_promise_then_internal` sets `[[PromiseIsHandled]]`).
`perform_await_then` had no remaining caller and was removed.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, empty script 7,484,842 / 7,485,497 (base / new) subtracted for
the net figures. base = `origin/main` at `029b06a7` (binary sha256 `9fdac928a458...`), new = the same tree plus this
change (`a86ac698eabc...`). The isolating scripts are this PR's reading of the issue's variants (6,000 requests;
1,500 iterations x 4 awaits); absolute numbers differ from the issue's, the ratios are what matter.

| workload | base Ir | new Ir | net delta | criterion |
|---|---:|---:|---:|---|
| `repro.js` pipeline (net) | 159,737,795 | 108,766,988 | -31.9% | >= 25% |
| `req.js` 6,000 `await it.next()` (net) | 149,702,608 | 114,486,586 | -23.5% | >= 15% |
| `await.js` 4 awaits per item (net) | 115,123,497 | 55,098,259 | -52.1% | >= 35% |
| `ctl-await-gen.js`, Ir per await | 15,241 | 7,097 | -53.4% | <= 10,000 |
| `ctl-await-fn.js`, Ir per await (async function control) | 5,399 | 5,413 | +0.3% | within +-2% |
| async-fs FULL | 11,045,689,488 | 9,916,426,069 | -10.2% | >= 8% |
| async-fs setup-only | 5,126,804,739 | 5,118,129,475 | -0.2% | unchanged |

async-fs scored iteration (FULL minus setup-only): 5,918,884,749 -> 4,798,296,594 Ir, -18.9%. callgrind
(`line-tables-only` + frame-pointer build of the same tree, call edges into `create_function_with_name`): base FULL
332,982 -> new 960 calls, setup-only 2,562 -> 960, so the scored iteration goes from 330,420 calls (195,502 from
`create_resolving_functions`, 136,520 from `async_gen_suspend_at_await`) to zero. Timed iteration of the async-fs
driver (`taskset` one core, alternating, shipped builds, load 4-5): base 1701/1706/1700/1705/1719 ms (median 1705),
new 1332/1336/1332/1338/1324 ms (median 1332), -21.9%.

### Controls (Ir, same binaries; "no Ir regression > 1%")

| control | base | new | delta |
|---|---:|---:|---:|
| `bench_promises_for_await` | 3,944,548,011 | 3,371,906,196 | -14.5% |
| `bench_promises_async_fs` | 11,048,353,722 | 9,921,777,286 | -10.2% |
| `bench_promises_await_loop` | 2,298,753,465 | 2,301,135,489 | +0.10% |
| `bench_promises_async_call` | 5,350,541,126 | 5,350,283,958 | 0.00% |
| `bench_promises_then_chain` | 3,384,860,063 | 3,385,978,292 | +0.03% |
| `bench_promises_new_promise` | 6,271,860,472 | 6,273,402,272 | +0.02% |
| `bench_promises_queue_deep` | 3,967,133,532 | 3,971,416,923 | +0.11% |
| `bench_generators_simple_yield` | 8,237,659,927 | 8,237,691,443 | 0.00% |
| `bench_generators_manual_next` | 5,399,186,806 | 5,399,188,293 | 0.00% |
| `bench_generators_pipeline` | 7,978,695,941 | 7,978,691,385 | 0.00% |
| `bench_generators_lazy_collections` | 8,807,325,629 | 8,807,209,191 | 0.00% |
| sync-fs (JetStream) | 12,904,733,732 | 12,902,849,769 | -0.01% |
| lazy-collections (JetStream) | 8,872,143,244 | 8,872,704,324 | +0.01% |

The +0.1% rows are the randomised identifier interner's run-to-run spread, not a code-path change (none of these
workloads touches an async generator).

## Correctness gates

Full test262 default and `--no-bytecode`: 99,911/99,911 each. test262-extra 1,416/1,416 normal and under
`JSSE_GC_STRESS=7`, on the plain build and on `release-checked` (root-stack asserts), in both modes. test262
AsyncGeneratorPrototype, AsyncFromSyncIteratorPrototype, async-generator statements/expressions, await, for-await-of and yield: 4,572/4,572 under
`JSSE_GC_STRESS=1` and `=7` on `release-checked`. `tests/` custom runner 19/19, `cargo test --lib` 1,103 passed,
`./scripts/lint.sh` clean.

New test262-extra tests: `async-generator-await-pending-then-settled-queue-order.js` (one generator awaiting a
pending promise, a settled promise, a non-object and a thenable, yielding pending and settled promises, with queued
`next`/`return`/`throw`, against an independent tick counter; expected log taken from node and identical on base),
`async-generator-request-result-thenable-assimilation.js` (`Object.prototype.then` observed, assimilating and
throwing against a request result) and `async-generator-await-continuation-runs-in-function-realm.js` (also pending/settled `yield` and `return(promise)`: the iterator result belongs to the generator's realm; base got this wrong for `yield <non-object>`, whose continuation ran in the draining realm, and the continuations now install the generator's realm).

## Not done

- `yield*` delegation and disposal await through `await_then` closures (unchanged, out of scope).
- The trailing `enqueue_microtask` closure in `async_gen_await_resume` (queue still non-empty after a request
  completes) is unchanged.
- Found while writing the matrix test, present before this change: `yield Promise.reject(x)` inside `try` is not
  catchable in the generator, now #1082 (the rejected-operand path retires the generator and rejects the request instead of
  throwing at the `yield`; node catches it).
