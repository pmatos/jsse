# #1023: a function's call-site caches live on its literal frame

After #1120 the inline-cache bookkeeping of a compiled call was the largest piece of the call path that is not the call
itself (fib, callgrind, Ir per call): `run_in_ic_body` 70, `IcStore::exit_body` 24 and `call_slot` 31 on top of the
handle validation in `enter_body_cached`. A function body found its `BodyIcStore` through the interpreter's `IcStore`
table every call: generation check, entry lookup, Body-identity check, recency clock tick and in-flight count on the way
in, an `entry_mut` with three asserts on the way out, and the same validation on every `call_slot` access.

## Change

- The `BodyIcStore` of a function literal is owned by its `LiteralFrame` (`FrameIcStore = Rc<BodyIcStore>`, created on
  first entry). Entering a body is one `Rc` clone into `Interpreter::current_ic`, a three-state `CurrentIc` (`None`,
  `Table(handle)` for script, module and eval bodies, `Frame(store)` for function bodies); leaving restores the previous
  value. Nothing is counted in flight and nothing can be evicted, because the store lives as long as the frame that hands it
  out. `IcStore::enter_body_cached` and the handle cell on the frame (#1062) are gone.
- Call slots are `Cell<CallIcSlot>` (the slot is `Copy`), read and written through `Interpreter::call_slot` /
  `set_call_slot` with no borrow flag; property slots (`Poly` owns a `Vec`) sit behind one `RefCell` and are reached through
  `with_prop_slot`. A first version with the whole store behind a `RefCell` cut instructions but not wall time: the
  borrow-flag read-modify-write on every `call_slot` is a dependent memory operation.
- The IC table keeps script, module and eval bodies, so its eviction bound still applies to them. Function bodies,
  including every `new Function`, no longer enter it; their stores are freed with their literal.

No `unsafe`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` `67dce67a` (binary sha256 `cf0ffba93d3de9...`),
new = base plus this change (`223957bf0e0c75...`). JetStream drivers at `--iterations 1`; geometric mean over the 31
workloads -1.10%.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| fib (micro) | 5,938,576,794 | 5,658,554,473 | -4.72% |
| delta-blue | 7,230,708,359 | 6,979,042,539 | -3.48% |
| lazy-collections | 4,525,509,355 | 4,397,051,128 | -2.84% |
| bigint-bigdenary | 4,886,382,481 | 4,769,295,025 | -2.40% |
| array (micro) | 782,135,660 | 763,539,297 | -2.38% |
| proxy-mobx | 645,635,362 | 632,710,783 | -2.00% |
| hash-map | 15,478,878,980 | 15,172,188,805 | -1.98% |
| richards | 11,476,361,812 | 11,266,398,587 | -1.83% |
| closures (micro) | 5,946,599,287 | 5,851,706,497 | -1.60% |
| Babylon | 2,916,089,646 | 2,870,289,938 | -1.57% |
| raytrace | 15,126,960,025 | 14,908,626,509 | -1.44% |
| earley-boyer | 5,646,044,768 | 5,569,986,056 | -1.35% |
| Air | 1,966,033,831 | 1,939,785,448 | -1.34% |
| bigint-noble-ed25519 | 7,336,529,059 | 7,241,476,987 | -1.30% |
| splay | 8,878,679,202 | 8,776,728,680 | -1.15% |
| ai-astar | 12,700,651,113 | 12,555,339,557 | -1.14% |
| doxbee-promise | 4,042,440,512 | 4,002,526,563 | -0.99% |
| promise_new (micro) | 5,921,805,533 | 5,880,389,113 | -0.70% |
| Box2D | 7,094,546,266 | 7,053,992,514 | -0.57% |
| opmix (micro) | 53,064,441,721 | 52,812,186,671 | -0.48% |
| Basic | 3,558,002,933 | 3,550,366,831 | -0.21% |
| stanford-crypto-sha256 | 4,537,645,306 | 4,530,599,741 | -0.16% |
| gaussian-blur | 27,017,269,705 | 26,978,022,782 | -0.15% |
| regex (micro) | 117,578,082 | 117,418,109 | -0.14% |
| navier-stokes | 3,844,591,582 | 3,843,177,342 | -0.04% |
| json (micro) | 314,100,843 | 314,040,537 | -0.02% |
| json-parse-inspector | 9,162,802,606 | 9,163,229,134 | +0.00% |
| string (micro) | 342,277,683 | 342,280,771 | +0.00% |
| object (micro) | 1,465,667,891 | 1,465,787,343 | +0.01% |
| crypto | 4,950,437,233 | 4,953,002,897 | +0.05% |
| js-tokens | 5,058,215,697 | 5,161,232,859 | +2.04% |

Reproducer (the issue's leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| shape | base | new | delta |
|---|---:|---:|---:|
| sloppy, 1 argument | 2,509 | 2,402 | -107 |
| strict, 1 argument | 2,488 | 2,381 | -107 |
| sloppy, 4 arguments | 2,938 | 2,831 | -107 |
| method call `o.inc(s)` | 2,960 | 2,853 | -107 |
| captured-variable closure call | 2,751 | 2,644 | -107 |
| `new P(i)` | 6,040 | 5,951 | -89 |
| sort comparator (64 elements, per iteration) | 13,890 | 13,421 | -469 |
| reads `arguments` (control) | 9,224 | 9,113 | -111 |
| call-free loop (control) | 734 | 734 | +0 |

Wall (hyperfine, 15 runs twice, pinned to one CPU, load average 1.5-11; medians of each pair): fib 1.126 s -> 1.045 s
(-7.2%), richards 2.832 s -> 2.757 s (-2.6%), delta-blue 1.696 s -> 1.630 s (-3.9%), hash-map 3.751 s -> 3.666 s (-2.3%).

## Correctness gates

`test262-extra/call-site-cache-follows-the-function-literal.js` warms a call site and then sends it a function, a Proxy, a
bound function, a class constructor and a non-callable, through two closures of one literal, then checks recursion
through a site and a callee whose shape changes. The ic_store unit tests cover a frame store shared by nested entries
(and restoring the outer store) and keep the table tests; the per-Body cache bound test now asserts function bodies stay
out of the IC table.

Full test262 default and `--no-bytecode`: 99,911/99,911 each; `test262-extra/` 1,480/1,480 normal, `--no-bytecode`, and both
under `JSSE_GC_STRESS=7`, and on the `release-checked` binary (both modes and stress); `cargo test --lib` 1,138 passed;
`perf_counters_*` integration tests pass; `tests/recursion-limit-*.js` 3/3 and `scripts/run-custom-tests.py` 19/19 on the
plain release build; `./scripts/lint.sh` clean.
