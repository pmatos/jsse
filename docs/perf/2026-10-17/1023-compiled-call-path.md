# #1023: serve an ordinary call of a compiled function from its literal frame

#1062 and #1063 moved the parameter atoms, the IC-store handle and the sloppy `fn.arguments` record off the per-call path.
What was left, re-profiled on `origin/main` (`01ace55b`, fib, callgrind with line tables, Ir per call): `call_function_inner_impl`
404 self (it builds a `CallTarget` of five `Rc` clones, swaps `calling_as_construct`/`new_target`, and carries the async and
generator arms in the same function), `dispatch_body` 146, `Environment::reset_scope` 134 (the pool reset it twice per
call), `pop_operands` 133, `run_chunk_with_var_prologue` 95, `Environment::get_this` 126 on richards, plus the compile
verdict, hoisting analysis and a `this` clone nobody reads.

The frame now keeps the hoisting analysis and compiled-call record alive for as long as any closure of the literal, so
the bound that `HoistCache` puts on cached Bodies (#165) no longer limits those entries for reachable functions; memory stays
proportional to the live function literals.

## Change

- **`CompiledCall` on the literal frame** (`LiteralFrame::compiled_call`, #1089 items 1, 3, 4). Once a call of a plain
  function (not generator/async, simple parameters, no `arguments` binding, not a class constructor) finds a `Compiled`
  verdict, the frame keeps the parameter list, `Body`, chunk and the arrow/strict flags. A later [[Call]] clones only the
  frame and the closure environment. The hoisting analysis moved to the frame as well (`LiteralFrame::hoist_analysis`), so
  neither the compiled nor the tree-walker path probes the Body-keyed memo per call.
- **`call_compiled_function`** is the ordinary-call tail of the general path without its constructs (the frame and environment
  push, tail-call and dispose bookkeeping, and the `fn.arguments` record are shared helpers, `run_activation` and
  `deferred_arguments_record`, so the two paths cannot drift): no NewTarget swap, no
  derived-`this` TDZ, no async or generator arm, and `dispatch_body`'s verdict match replaced by the shared
  `run_compiled_body`. It runs only when `!calling_as_construct && new_target.is_none() && !constructing_derived`, so
  constructions, tail-call chains inside constructors and `arguments`-reading functions keep the general path,
  now `call_function_general`. A Proxy or wrapped function copies its target's callable (and frame), and a class constructor throws when
  called, so the fast path checks the object for those every time.
- **One environment reset per call.** A pooled environment is released to a canonical state (`Environment::release_to_pool`)
  and acquisition only sets the parent, `strict` and function-scope flag (`adopt_pooled`); a debug assertion checks the
  canonical state on every acquisition. Block and function environments share `recycle_environment`.
- **Batched unroot.** `pop_operands` releases the call operands' roots with one `RootStack::pop_expected_batch`: while the top
  roots match the operands' ids it walks an index and truncates once, and a mismatch falls back to `pop_expected`.
- **`LoadThis`** answers from the frame environment's own `this` binding (block environments never carry one) before
  walking the scope chain.
- Smaller: `run_chunk` lost its unused `this` parameter (no more `JsValue` clone per call), the `refs` buffer is not pooled
  when the chunk has no reference slots.

No `unsafe` is added; the `DeferredCallArguments` constructor from #1063 keeps its single call site (now two, same invariant).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` `01ace55b` (binary sha256 `4e8618fea9dfa4...`),
new = base plus this change (`cf0ffba93d3de9...`). JetStream drivers at `--iterations 1`.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| fib (micro) | 6,907,891,002 | 5,938,576,794 | -14.03% |
| delta-blue | 8,115,026,548 | 7,230,708,359 | -10.90% |
| array (micro) | 850,369,305 | 782,135,660 | -8.02% |
| hash-map | 16,798,932,910 | 15,478,878,980 | -7.86% |
| richards | 12,423,758,576 | 11,476,361,812 | -7.63% |
| bigint-bigdenary | 5,257,105,172 | 4,886,382,481 | -7.05% |
| proxy-mobx | 690,834,348 | 645,635,362 | -6.54% |
| Babylon | 3,080,501,956 | 2,916,089,646 | -5.34% |
| Air | 2,064,530,390 | 1,966,033,831 | -4.77% |
| lazy-collections | 4,727,886,613 | 4,525,509,355 | -4.28% |
| splay | 9,254,269,613 | 8,878,679,202 | -4.06% |
| raytrace | 15,749,322,884 | 15,126,960,025 | -3.95% |
| ai-astar | 13,102,358,770 | 12,700,651,113 | -3.07% |
| earley-boyer | 5,811,182,096 | 5,646,044,768 | -2.84% |
| Basic | 3,654,602,784 | 3,558,002,933 | -2.64% |
| Box2D | 7,285,234,611 | 7,094,546,266 | -2.62% |
| bigint-noble-ed25519 | 7,523,314,082 | 7,336,529,059 | -2.48% |
| closures (micro) | 6,085,341,941 | 5,946,599,287 | -2.28% |
| opmix (micro) | 54,233,211,514 | 53,064,441,721 | -2.16% |
| js-tokens | 5,128,575,599 | 5,058,215,697 | -1.37% |
| crypto | 5,013,142,889 | 4,950,437,233 | -1.25% |
| doxbee-promise | 4,088,926,249 | 4,042,440,512 | -1.14% |
| gaussian-blur | 27,245,352,710 | 27,017,269,705 | -0.84% |
| stanford-crypto-sha256 | 4,570,437,048 | 4,537,645,306 | -0.72% |
| navier-stokes | 3,867,315,008 | 3,844,591,582 | -0.59% |
| string (micro) | 343,182,806 | 342,277,683 | -0.26% |
| object (micro) | 1,466,719,421 | 1,465,667,891 | -0.07% |
| json-parse-inspector | 9,159,585,824 | 9,162,802,606 | +0.04% |
| json (micro) | 313,585,440 | 314,100,843 | +0.16% |
| regex (micro) | 117,168,421 | 117,578,082 | +0.35% |
| promise_new (micro) | 5,885,240,170 | 5,921,805,533 | +0.62% |

Call counts are `acquire_function_environment` call edges from callgrind: fib 2,692,537, richards 2,024,357, delta-blue 2,266,466.
Whole-process Ir per fib call: 2,768 on `252429a7` (before #1119), 2,566 on `01ace55b`, 2,206 with this change.

Reproducer (the issue's leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| shape | base | new | delta |
|---|---:|---:|---:|
| sloppy, 1 argument | 2,868 | 2,509 | -359 |
| strict, 1 argument | 2,847 | 2,488 | -359 |
| sloppy, 4 arguments | 3,312 | 2,938 | -374 |
| method call `o.inc(s)` | 3,340 | 2,960 | -380 |
| captured-variable closure call | 3,110 | 2,751 | -359 |
| `new P(i)` | 6,108 | 6,040 | -68 |
| sort comparator (64 elements, per iteration) | 15,814 | 13,890 | -1,924 |
| reads `arguments` (control, general path) | 9,166 | 9,224 | +58 |
| call-free loop (control) | 735 | 734 | -1 |

Sloppy minus strict stays at 21 Ir.

Wall (hyperfine, 9 runs, pinned to one CPU; medians): fib 1.322 s -> 1.116 s (-15.6%), richards
3.077 s -> 2.821 s (-8.3%), delta-blue 1.925 s -> 1.703 s (-11.5%), hash-map 4.117 s -> 3.753 s (-8.8%).

## Correctness gates

`test262-extra/call-compiled-function-repeated-calls.js` calls each function repeatedly, so the later calls take the new
path: sloppy/strict `this` coercion, `Function.prototype.arguments` and `caller`, a class constructor called without
`new` after being constructed, a Proxy (and a revoked Proxy) over a warmed-up function, captured parameters, hoisted
declarations, missing/extra arguments, strict tail calls, exceptions with `finally`, catchable stack exhaustion followed
by normal calls, a function from another realm, and arguments surviving `$262.gc()`.
`test262-extra/call-frame-pooled-environment-reuse.js` covers recycled environments (stale arrow/strict/`arguments`/function
scope flags, block scopes released before a function scope is acquired, `this` in nested blocks, derived-constructor TDZ,
operands rooted across a collection and across a throw). With the proxy check removed the first test fails.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,479/1,479 normal, `--no-bytecode`, and both
under `JSSE_GC_STRESS=7`, and on the `release-checked` binary (both modes and stress); `cargo test --lib` 1,140 passed;
the `perf_counters_*` integration tests pass (`perf_counters_function_creation` is the known flake #1078);
`tests/recursion-limit-*.js` 3/3 and `scripts/run-custom-tests.py` 19/19 on the plain release build;
`./scripts/lint.sh` clean. The hoist-cache unit test now asserts the analysis is read off the frame instead of counting memo hits.
