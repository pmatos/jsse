# #1105 stage 2: async generator bodies as suspending bytecode chunks

An async generator whose body the compiler can host no longer runs as a state machine: the body is one chunk that
suspends at `yield` and `await`, and the request queue is served by `async_gen_drain` (ADR-2026-10-19-2310).
`yield*` and `for await` keep the state machine (next stage).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `1a9f320e` (stage 1, #1148); new = this change. cachegrind `--cache-sim=no`, whole process,
`--iterations 1` JetStream drivers.

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| async-fs | 7,716,170,850 | 5,403,395,141 | -30.0% |
| doxbee-async | 1,433,201,901 | 1,431,780,786 | -0.1% |
| doxbee-promise | 2,333,129,242 | 2,325,923,875 | -0.3% |
| promise_new (micro) | 5,922,933,850 | 5,921,234,420 | -0.03% |
| fib (micro) | 5,048,746,367 | 5,048,746,485 | 0.00% |
| opmix (micro) | 40,698,013,417 | 40,698,012,978 | 0.00% |
| richards | 8,780,400,548 | 8,780,364,607 | 0.00% |
| delta-blue | 6,430,022,015 | 6,430,040,533 | 0.00% |
| Box2D | 5,587,637,942 | 5,588,516,282 | +0.02% |
| lazy-collections | 3,796,657,902 | 3,798,151,480 | +0.04% |
| bigint-noble-secp256k1 | 4,842,842,547 | 4,854,563,662 | +0.24% |
| gaussian-blur | 20,665,746,671 | 20,665,025,785 | 0.00% |
| splay | 7,810,139,630 | 7,809,775,237 | 0.00% |

Wall (`wall.py`: interleaved, pinned to one CPU, 9 runs, median, load 1.6-2.7, shipped binaries; base-rel is
stage 1's parent `8fffeb0a`, s1 is stage 1):

| benchmark | `8fffeb0a` | stage 1 | new |
|---|---:|---:|---:|
| async-fs | 1.767 s | 1.811 s | 1.401 s (-20.7% vs `8fffeb0a`) |
| doxbee-async | 0.784 s | 0.566 s | 0.574 s |
| richards | 1.943 s | 1.935 s | 1.905 s |
| Box2D | 1.233 s | 1.219 s | 1.211 s |
| splay | 2.050 s | 2.064 s | 2.051 s |
| delta-blue | 1.392 s | 1.384 s | 1.371 s |
| fib | 0.928 s | 0.931 s | 0.918 s |
| lazy-collections | 0.876 s | 0.873 s | 0.854 s |
| gaussian-blur | 3.966 s | 3.979 s | 3.971 s |
| bigint-noble-secp256k1 | 1.114 s | 1.114 s | 1.123 s |

## Net Ir per yield (fixtures; N=30k and N=90k; the consumer is a hosted async function calling `await it.next()`)

| fixture | base | new |
|---|---:|---:|
| `yield i` | 11,851 | 8,929 |
| 4 arithmetic statements + `yield` | 18,827 | 9,865 |
| `yield await i` | 14,272 | 10,024 |
| `yield` inside two nested try/finally | 21,759 | 9,850 |
| same generator consumed by a `for await` (state-machine consumer) | 19,078 | 16,153 |

What remains per yield is the request machinery (the request promise, the iterator result, its resolution and the
consumer's await), which this stage leaves as it was.

## A layout trap

A second `IteratorState` variant (`BytecodeAsyncGenerator`) instead of the `is_async` flag, with identical
behaviour, left every Ir count flat and slowed Box2D, splay, gaussian-blur, delta-blue and fib by 0.5-1.2% wall in
three measurement sessions; declaring the new modules at the end of their parents did not change it, and dropping the
variant (no edit to the GC trace match) did. The flag is therefore also the measured choice, not only the smaller one.

## Not changed

`for await` consumers (`runIteration`, `totalFileSize`, `visit` in async-fs) and `yield*` generators
(`forEachFile*`) stay on the state machine, which is most of what is left of async-fs.
