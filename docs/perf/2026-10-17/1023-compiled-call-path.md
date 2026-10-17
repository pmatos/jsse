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
- **`call_compiled_function`** is the ordinary-call tail of the general path without its constructs: no NewTarget swap, no
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
new = base plus this change (`976faa894aba3d...`). JetStream drivers at `--iterations 1`.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| fib (micro) | 6,907,891,002 | 5,945,309,121 | -13.93% |
| delta-blue | 8,115,026,548 | 7,228,378,844 | -10.93% |
| hash-map | 16,798,932,910 | 15,431,473,135 | -8.14% |
| array (micro) | 850,369,305 | 782,337,901 | -8.00% |
| richards | 12,423,758,576 | 11,440,150,893 | -7.92% |
| bigint-bigdenary | 5,257,105,172 | 4,873,282,267 | -7.30% |
| proxy-mobx | 690,834,348 | 644,546,384 | -6.70% |
| Babylon | 3,080,501,956 | 2,908,487,822 | -5.58% |
| Air | 2,064,530,390 | 1,963,156,955 | -4.91% |
| lazy-collections | 4,727,886,613 | 4,512,188,827 | -4.56% |
| raytrace | 15,749,322,884 | 15,063,870,906 | -4.35% |
| splay | 9,254,269,613 | 8,863,723,361 | -4.22% |
| ai-astar | 13,102,358,770 | 12,646,422,573 | -3.48% |
| Box2D | 7,285,234,611 | 7,052,858,361 | -3.19% |
| earley-boyer | 5,811,182,096 | 5,628,263,219 | -3.15% |
| opmix (micro) | 54,233,211,514 | 52,562,287,205 | -3.08% |
| bigint-noble-ed25519 | 7,523,314,082 | 7,301,512,090 | -2.95% |
| Basic | 3,654,602,784 | 3,558,139,374 | -2.64% |
| crypto | 5,013,142,889 | 4,883,546,217 | -2.59% |
| closures (micro) | 6,085,341,941 | 5,950,649,721 | -2.21% |
| gaussian-blur | 27,245,352,710 | 26,705,240,660 | -1.98% |
| stanford-crypto-sha256 | 4,570,437,048 | 4,487,532,766 | -1.81% |
| navier-stokes | 3,867,315,008 | 3,801,916,583 | -1.69% |
| doxbee-promise | 4,088,926,249 | 4,032,775,292 | -1.37% |
| string (micro) | 343,182,806 | 341,079,339 | -0.61% |
| js-tokens | 5,128,575,599 | 5,107,328,897 | -0.41% |
| object (micro) | 1,466,719,421 | 1,465,065,822 | -0.11% |
| json-parse-inspector | 9,159,585,824 | 9,160,003,019 | +0.00% |
| json (micro) | 313,585,440 | 313,983,787 | +0.13% |
| regex (micro) | 117,168,421 | 117,344,538 | +0.15% |
| promise_new (micro) | 5,885,240,170 | 5,913,074,154 | +0.47% |

Call counts are `acquire_function_environment` call edges from callgrind: fib 2,692,537, richards 2,024,357, delta-blue 2,266,466.
Whole-process Ir per fib call: 2,768 on `252429a7` (before #1119), 2,566 on `01ace55b`, 2,208 with this change.

Reproducer (the issue's leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| shape | base | new | delta |
|---|---:|---:|---:|
| sloppy, 1 argument | 2,868 | 2,506 | -362 |
| strict, 1 argument | 2,847 | 2,485 | -362 |
| sloppy, 4 arguments | 3,312 | 2,932 | -380 |
| method call `o.inc(s)` | 3,340 | 2,952 | -388 |
| captured-variable closure call | 3,110 | 2,747 | -363 |
| `new P(i)` | 6,108 | 6,051 | -57 |
| sort comparator (64 elements, per iteration) | 15,814 | 13,905 | -1,909 |
| reads `arguments` (control, general path) | 9,166 | 9,220 | +54 |
| call-free loop (control) | 735 | 724 | -11 |

Sloppy minus strict stays at 21 Ir.

Wall (hyperfine, 9 runs, pinned to one CPU, load average 3-5; medians): fib 1.330 s -> 1.117 s (-16.0%), richards
3.074 s -> 2.800 s (-8.9%), delta-blue 1.919 s -> 1.715 s (-10.6%), hash-map 4.137 s -> 3.808 s (-8.0%).

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
