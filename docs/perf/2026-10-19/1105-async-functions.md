# #1105 stage 1: async function bodies as suspending bytecode chunks

An async function whose body the compiler can host no longer runs as a state machine over tree-walked payloads: the
body is one chunk, `await` suspends it (`Op::Yield` with a zero skip offset), and the promise reaction resumes it
(ADR-2026-10-19-2030). Async generators, `yield*` and `for await` keep the state machine (later stages).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `8fffeb0a` (line-tables + frame-pointer binary for Ir, plain release for wall/RSS); new = this
change. cachegrind `--cache-sim=no`, whole process, `--iterations 1` JetStream drivers.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| doxbee-async | 2,050,002,084 | 1,432,772,175 | -30.1% |
| async-fs (for await / async generators dominate) | 7,727,655,896 | 7,709,530,530 | -0.2% |
| doxbee-promise (no async functions) | 2,326,300,228 | 2,329,332,778 | +0.13% |
| promise_new (micro) | 5,919,869,863 | 5,923,962,988 | +0.07% |
| fib (micro) | 5,048,745,488 | 5,048,745,704 | 0.00% |
| opmix (micro) | 40,698,009,439 | 40,698,012,084 | 0.00% |
| richards | 8,780,383,829 | 8,780,373,498 | 0.00% |
| delta-blue | 6,429,766,269 | 6,431,225,845 | +0.02% |
| Box2D | 5,581,412,977 | 5,595,560,583 | +0.25% |
| lazy-collections | 3,781,439,532 | 3,782,294,394 | +0.02% |
| bigint-noble-secp256k1 | 4,846,890,548 | 4,826,377,470 | -0.42% |
| gaussian-blur | 20,664,663,785 | 20,665,744,416 | +0.01% |
| splay | 7,810,775,034 | 7,809,189,624 | -0.02% |

`vm.rs` is untouched in `run_chunk_inner`. Both drivers call the one `run_generator_chunk`, now `#[inline(never)]`: leaving
it to LTO gave lazy-collections +0.41% (two call sites), `#[inline(always)]` +1.26%, and a separate async wrapper moved
fib/richards wall +2-3% at equal Ir (dispatch-loop layout). With the out-of-line shared wrapper the sync generator path
pays one call per resume (lazy-collections +0.41% Ir, wall neutral) and fib/richards/Box2D/splay/delta-blue are flat.

Wall (`wall.py`: interleaved, pinned to one CPU, 9 runs, median, load 1.7-2.8, shipped binaries):

| benchmark | base | new |
|---|---:|---:|
| richards | 1.933 s | 1.917 s |
| Box2D | 1.228 s | 1.210 s |
| splay | 2.059 s | 2.060 s |
| delta-blue | 1.394 s | 1.379 s |
| fib | 0.928 s | 0.933 s |
| lazy-collections | 0.876 s | 0.877 s |
| doxbee-async | 0.782 s | 0.569 s (-27%) |
| async-fs | 1.769 s | 1.809 s (+2.3%) |

async-fs wall is +1.5-2.3% in every build variant tried (also with hosting switched off at run time) while its Ir is
-0.1..-0.2% and its simulated I1 misses and branch mispredicts are lower: the state-machine code it runs is unchanged,
so this is code placement, not work. The next stages move its hot loops off that code.

Peak RSS (`/usr/bin/time`, two runs each, base / new): doxbee-async 94.9-95.0 MB / 87.9-88.2 MB, async-fs 28.4-30.2 MB /
28.7 MB, richards 19.5 MB / 19.0-19.3 MB, splay 593-597 MB / 596-598 MB, Box2D 39.0-40.6 MB / 39.1 MB, lazy-collections
27.8-28.0 MB / 27.7-27.9 MB, delta-blue 23.2-23.7 MB / 23.2 MB.

## Net Ir per iteration (fixtures; N=30k and N=90k, consumer inside the async function)

| fixture | base | new |
|---|---:|---:|
| `s += await i` (primitive) | 7,466 | 1,612 |
| `s += await Promise.resolve(i)` | 11,175 | 4,526 |
| 4 arithmetic statements + `await i` | 17,189 | 2,639 |
| `await` inside two nested try/finally | 16,585 | 2,496 |
| `await leaf(i)`, leaf an async function | 13,694 | 6,257 |
| call of an async function that never awaits | 5,808 | 4,755 |

## Where the Ir went

A state step paid the driver's per-resume bookkeeping (`reconcile_scope_stack`, for-of stack copies, the terminator
operand through `eval_operand`, payload through `exec_body_inner`, string-keyed name lookup). Hosted, a resume is
`resume_hosted_async` + one `run_async_chunk`; the rest is the promise machinery, which is unchanged
(`await_promise_resolve`, one reaction job per await), so tick counts are identical (checked against node in
`test262-extra/async-function-bytecode-await-tick-counts.js`).

## Not changed

`async-fs` is dominated by `for await` loops and async generators (its untimed `randomFileContents` fill loop is an
async generator body), so it needs the next stages.
