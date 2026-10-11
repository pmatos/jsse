# promises — Promise / async-await / microtask performance

jsse HEAD `2b621c5` vs boa 0.22.0, buildbox3, 2026-10-10. Repros: `bench_promises_*.js` (9 files) and `experiments.patch` (throw-away patches A-D below; `git apply --check` passes on HEAD 2b621c5) in this directory. All nine repros print the same checksum on jsse and boa.

## Summary

The three target benchmarks are Promise-machinery bound; `--no-bytecode` is identical on every repro. Throw-away patches 1+2+3 give:

| Benchmark | Before | After | vs boa |
|---|---|---|---|
| doxbee-async | 2.73 s | 1.32 s | 4.4x → 2.1x |
| doxbee-promise (N=5000) | 4.58 s | 2.07 s | 4.1x → 1.85x |
| doxbee-promise (N=10000) | 13.2 s | 5.2 s | 5.8x → 2.25x |

## Gap table

Median of 5 runs. Rows marked * were re-run at load ~4 because the first pass ran at load 12-14.

| repro | jsse s | boa s | ratio | jsse --no-bytecode s | loadavg |
|---|---|---|---|---|---|
| queue_deep (20k chains × 6 thens) | 9.90 | 1.07 | 9.3x | 10.06 | 9.9 |
| doxbee_async (N=5000)* | 2.73 | 0.62 | 4.4x | 2.68 | 4.4 |
| doxbee_promise (N=5000)* | 4.58 | 1.12 | 4.1x | 4.53 | 4.1 |
| then_chain (120k thens) | 4.58 | 1.02 | 4.5x | 4.43 | 8.5 |
| async_fs (1 iteration, JetStream source verbatim)* | 8.40 | 2.27 | 3.7x | 8.40 | 5.1 |
| async_call (200k async calls)* | 6.14 | 1.72 | 3.6x | 6.09 | 4.3 |
| new_promise (600k promises) | 5.94 | 1.82 | 3.3x | 5.64 | 10.4 |
| for_await (100k items) | 3.78 | 1.72 | 2.2x | 3.78 | 10.6 |
| await_loop (180k awaits)* | 1.92 | 0.97 | 2.0x | 1.92 | 4.6 |

Scaling, single runs at load 3-5:

| Repro | Size | jsse s | boa s |
|---|---|---|---|
| doxbee_promise | N=2500 | 1.77 | 0.50 |
| doxbee_promise | N=5000 | 4.50 | 1.02 |
| doxbee_promise | N=10000 | 13.22 | 2.29 |
| then_chain | 60k thens | 1.76 | 0.49 |
| then_chain | 120k thens | 4.51 | 1.00 |
| then_chain | 240k thens | 14.17 | 1.90 |

jsse grows ×2.5-3.1 per doubling; boa ×1.9-2.2. At N=10000 the ratio is 5.8x, matching JetStream's 6.18x.

Experiment builds (3 runs, load 6-10, cumulative): A = VecDeque queue; B = A + async state-machine cache; C = B + `then()` without resolve/reject objects; D = C + `Promise.resolve` fast path.

| repro | head | A | B | C | D | boa |
|---|---|---|---|---|---|---|
| queue_deep | 9.90 | 6.19 | 6.14 | 3.33 | - | 1.07 |
| then_chain | 4.58 | 4.58 | 4.58 | 2.32 | - | 1.02 |
| doxbee_promise N=5000 | 4.58 | 3.43 | 3.43 | 2.47 | 2.07 | 1.12 |
| doxbee_promise N=10000 | 13.22 | 8.51 | - | 6.24 | 5.16 | 2.29 |
| doxbee_async | 2.73 | 2.58 | 1.32 | 1.32 | - | 0.62 |
| async_call | 6.14 | 5.80 | 5.09 | 3.93 | 3.98 | 1.72 |
| await_loop | 1.92 | 1.97 | 1.67 | 1.62 | 1.42 | 0.97 |
| for_await | 3.78 | 3.78 | 3.23 | 3.08 | - | 1.72 |
| async_fs | 8.40 | 8.45 | 7.80 | 7.65 | - | 2.27 |
| new_promise | 5.94 | 5.99 | 6.04 | 5.94 | 4.93 | 1.82 |

The patches are crude lower bounds, not designs.

## Root causes (ranked)

**RC1. Microtask dequeue is O(depth).**
- `scheduler.rs:191` declares `Vec<(Vec<JsValue>, Box<dyn FnOnce>)>`, 40 bytes per element. `pop_microtask` (`scheduler.rs:213-219`) calls `remove(0)`, so each job memmoves 40·depth bytes. Drain loops: `mod.rs:5450`, `mod.rs:5698`, `eval.rs:10301`.
- doxbee starts N chains synchronously, so the queue stays ~N deep and the cost is quadratic.
- Callgrind at N=1000: `__memcpy_avx_unaligned_erms` is 30.2% of instructions in doxbee-async (26.6% attributed to `drain_microtasks`) and 47.8% in doxbee-promise.
- Residual after A: queue_deep is still 6.19 s against then_chain's 4.58 s at equal job count. Cause UNVERIFIED (`for_each_root` walking queued roots, or cache effects).

**RC2. The async state machine is rebuilt per call.**
- `eval.rs:8007` calls `transform_async_function` on every invocation. `rewrite_stmts_await_to_yield` (`generator_transform.rs:3936`) deep-clones the AST; `transform_generator_inner_opts` (`:621`) then runs analysis and transform; `create_simple_machine` (`:681-690`) does `body.to_vec()` plus `assign_ic_sites_for_body`. Nothing is cached.
- Same pattern for async generators (`eval.rs:5397`) and generators (`eval.rs:5589`).
- Callgrind inclusive: 22.2% (doxbee-async N=1000), 8.0% (await_loop), `transform_generator_inner_opts` 7.0% in the small async-fs tree.
- Fresh `Body` identities per call mean IC and hoist side tables keyed by `Body::key()` never warm up. UNVERIFIED: size and growth of those tables.

**RC3. Every internal promise builds a resolve/reject pair of full function objects.**
- `create_resolving_functions` (`promise.rs:775-853`) is called from `promise_then` via `new_promise_capability` (`:20-45`, `:1140`), `promise_resolve_with_constructor` (`:159-187`), `call_async_function` (`eval.rs:7897-7906`), `promise_resolve_thenable` (`:978`, again per thenable job) and `Promise.all` elements.
- Each pair is 2x `create_function` (`mod.rs:1799-1960`: `class_name` String, key interning, `function_realm_map` insert), 2x `pin_native_root` (`gc.rs:328`), and a push to `resolving_functions`. For intrinsic `%Promise%` these are never observable.
- Callgrind on a 20k then chain:

  | Function | Inclusive |
  |---|---|
  | `promise_then` | 30.7% |
  | `new_promise_capability` | 21.1% |
  | `create_resolving_functions` | 17.2% |
  | `create_function` | 22.7% |
  | `insert_property` | 13.3% |
  | `intern_js_key` | 6.2% |
  | `species_constructor` | 5.7% |
  | `get_global_var("Promise")` (twice per `then`) | 3.4% |

- RSS: 1.37 GB for 240k thens in jsse against 385 MB in boa, ~5.7 KB per `then()`.
- The handler-returns-promise path adds a second pair, a job and another derived promise per stage (`promise_resolve_thenable` closure is 5.7% of doxbee-promise).
- doxbee-async gains nothing from C because async results still build the pair in `call_async_function`.

**RC4. GC is a large, super-linear share of promise workloads.**
- GC share of wall time (instrumented build, not timed against head): then_chain 120k 58%, doxbee_promise 36%, queue_deep 29%, new_promise 21%, doxbee_async 15%, async_fs 12%.
- then_chain `gc_major` goes 11 → 20 → ~40 for 60k/120k/240k, so majors scale linearly with allocation while each marks a growing, fully live heap.
- Cause read from `gc.rs:73-180`, not confirmed by experiment here (see the alloc-gc theme, where removing the rule was A/B tested on splay): after two ≥90%-survival minors, the pacer requests a major every `GC_NURSERY_THRESHOLD_BYTES × GC_SATURATED_MAJOR_NURSERIES` = 4 MiB (~8192 objects) regardless of live size (`charge_object`, `gc.rs:103-112`).
- `drop_glue::<Interpreter>` at exit is 13.1% of instructions in the 20k then chain and 3% in doxbee-promise (wall time only, probably not in the JetStream timer).

**RC5. Per-await and per-resume churn.**
- `async_function_resume` (`eval.rs:8078-8200`) removes the `AsyncFunctionState` from an `FxHashMap`, then re-inserts it with clones of `state_machine`, `func_env`, `try_stack`, `for_of_stack`, `scope_stack` and `module_path` (`:8164-8180`); inserts again at suspension (`eval.rs:10030-10066`).
- Each pending await creates 2 native function objects, `asyncFnFulfill`/`asyncFnReject` (`eval.rs:10100-10120`). Settled awaits enqueue a boxed closure plus a 3-element roots `Vec`.
- `promise_resolve_value` (`promise.rs:1239`) does a string-keyed `Get(constructor)` and a SipHash `get_global_var("Promise")`.
- await_loop is still 1.42 s against boa's 0.97 s after B+C+D.

**RC6. Cross-cutting costs (other themes).**
- Async bodies run on the tree-walker (`exec_body_inner` under `async_function_resume`). doxbee-async: 90000 AST units over 32000 executions.
- `Environment.bindings` is a std `HashMap<String, _>` with SipHash (`types.rs:847`); `hash_one::<&str>` is 15.5% (await_loop), 12.9% (async_fs), 11% (new_promise).
- `bytecode_cache` is on the function object (`eval.rs:1811-1842`), so every closure instance recompiles. then_chain: `compile_ok = 120001` for 120k instances of `v => v + 1`; doxbee-promise N=5000: 20013 compiles and 20003 bails.
- The bytecode compiler bails on `let`/`const` (`compiler.rs:553`) and object literals (`:1013`): 4002 and 4000 bails per 1000 doxbee-promise chains.
- Native `create_function` costs ~4150 instructions per object.

## Proposed work items

All must keep `run-test262.py` and `run-custom-tests.py` green.

**W1. `perf(scheduler): O(1) microtask dequeue`** (S)
- Scope: `microtask_queue` becomes a `VecDeque`; check `for_each_root` and the `microtask_queue_*` tests.
- Acceptance: no memcpy caller `drain_microtasks` in callgrind at N=1000; queue_deep ≤ 6.5 s; doxbee-promise N=10000 ≤ 9 s; ≤ ×2.6 per doubling.

**W2. `perf(async): cache async/generator state machines per function body`** (M)
- Scope: build the `Rc<GeneratorStateMachine>` once per literal (lazy, next to `Body`, or a side table keyed by `Body::key()` that pins the `Rc`). Apply to `eval.rs:8007`, `:5397`, `:5589`; coordinate with the generators theme (same item as generators #2).
- Acceptance: `transform_async_function` absent from the top-40 callgrind entries; doxbee-async ≤ 1.5 s.
- Risks: add test262-extra cases for concurrently suspended calls and recursion. `experiments.patch` hunk B drops the `#[cfg(feature = "perf-counters")]` `perf_key` assignment; keep it, or async bodies lose BODY attribution.

**W3. `perf(promise): unexposed capabilities without function objects`** (M-L)
- Scope: reaction target becomes `Capability | Internal(promise_id) | None` when the constructor is `%Promise%`. Covers then/catch/finally, `PromiseResolve`, the async result promise (`call_async_function`), and the thenable job for a native promise with the original `then`. Keep the `SpeciesConstructor` lookups; skip the native species getter call when it is the original.
- Acceptance: then_chain ≤ 2.5 s and 240k-chain RSS ≤ 700 MB; queue_deep ≤ 3.6 s; doxbee-promise N=5000 ≤ 2.2 s; doxbee-async ≤ 1.2 s; no `create_resolving_functions` under `promise_then`.
- Risks: job ordering must not change. The derived promise stays GC-reachable via `PromiseReaction.promise_id` (`gc.rs:1104`). Gate on the GC tests in `interpreter/tests.rs:1372-1560` and test262 `built-ins/Promise` and `language/expressions/await`.

**W4. `perf(async): typed jobs and reactions`** (M)
- Scope: an enum job type replaces `Box<dyn FnOnce>` plus the roots `Vec`. A typed `AsyncResume{async_id}` handler replaces the two native functions per await. Resume state in place. Skip `promise_resolve_value` allocation for pristine native promises.
- Acceptance: await_loop ≤ 1.2 s; for_await ≤ 2.6 s; `release-checked` and `JSSE_GC_STRESS=7` stay green.
- Risk: the root-frame discipline in `drain_microtasks` (`mod.rs:5450-5460`) changes.

**W5. `perf(gc): scale the suppressed-nursery major probe with live bytes`** (M, GC theme — same rule as alloc-gc WI-1)
- Scope: `GcPacer::charge_object` at `gc.rs:103-112`.
- Acceptance: then_chain ≤ ×2.2 per doubling; GC share ≤ 25%; no splay/richards regression; stay under the 512 MB test limit.

**W6. `perf(bytecode): share compiled chunks per function literal`** (M, calls theme — same item as calls WI-3)
- Scope: key `BytecodeCacheState` on `Body::key()`; follow-up: compile `let`/`const` and object literals.
- Acceptance: compile attempts equal distinct literals executed.

**W7. `perf(interp): cheap native function objects`** (S-M, same as calls WI-4)
- Scope: no `class_name` String, pre-interned keys, shared name `JsString`, skip the `function_realm_map` insert in the default realm.
- Acceptance: new_promise ≤ 4 s on top of W3.

**W8. `perf(cli): skip arena drop at exit`** (S)
- Scope: `mem::forget` the `Interpreter` in `cli::execute_code`.
- Acceptance: `drop_glue::<Interpreter>` absent from callgrind.

## What did NOT reproduce or is unverified

- **Timer/event-loop polling between jobs:** not a cost. The mutex is touched every 64 jobs and timers only after the drain (`mod.rs:5470-5500`, `:5608`); no related symbol in any profile.
- **Bytecode VM effect:** `--no-bytecode` equals default within noise on all nine repros.
- **Per-job root push/pop and safepoints:** safepoints are statement-granular. `gc_root_*` is inlined and never appears as a separate symbol. `gc_safepoint` inclusive is 5-15% but includes the collection work itself, so rooting overhead is UNVERIFIED.
- **Await tick counts:** not investigated.
- **`Promise.all` itself:** linear in element count; cost is one `then()` plus one function per element (RC3).

## Reproduction

```sh
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
J=$R/jsse-head/target/release/jsse; B=$R/boa-v0.22-src/target/release/boa
python3 $R/microbench.py "{\"jsse\":[\"$J\"],\"boa\":[\"$B\"],\"jsse_nobc\":[\"$J\",\"--no-bytecode\"]}" \
  docs/perf/2026-10-10/repros/promises /tmp/promises.json 5
sed 's/const N = 5000, ROUNDS = 1;/const N = 1000, ROUNDS = 1;/' bench_promises_doxbee_async.js > da.js
valgrind --tool=callgrind --callgrind-out-file=cg.out $J da.js && callgrind_annotate --inclusive=yes cg.out | head -60
cargo +nightly build --release --features perf-counters   # counters only, never time it
```

Remote raw data is under `$R/work-promises/`: `main.json`, `exp.json`, `expD.json`, `rerun.json`, and `bin/jsse-{A,B,C,D}`.
