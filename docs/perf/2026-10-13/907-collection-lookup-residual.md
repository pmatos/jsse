# #907 — residual after hashed Map/Set/WeakMap/WeakSet storage

`before` is `main` at `ffd59cf1`; `after` adds the compile-verdict change below.
Release builds, `boa` is boa 0.22 (`~/.cargo/bin/boa`). Repros are the five
files in the issue (`bench_proxy-classes_map_lookup.js`,
`bench_proxy-classes_weakmap_lookup.js`, `bench_generators_map_set_iteration.js`,
`bench_alloc-gc_native_map.js`, `bench_alloc-gc_map_rolling.js`).

The host was at load average 40-90 while measuring, so the figures are the
CPU time (user+sys) of the median of 5 interleaved runs; wall time swung by
2x. Treat differences under ~10% as noise.

## Acceptance criteria, measured

| repro | jsse before | jsse after | boa | after / boa |
|---|---:|---:|---:|---:|
| map_lookup | 0.127 s | 0.126 s | 0.107 s | 1.18x |
| weakmap_lookup | 0.144 s | 0.147 s | 0.129 s | 1.14x |
| map_set_iteration | 0.091 s | 0.090 s | 0.123 s | 0.73x |
| native_map | 0.068 s | 0.064 s | 0.232 s | 0.28x |
| map_rolling | 0.078 s | 0.077 s | 0.248 s | 0.31x |

All five are at or under 1.2x boa. The 2.1-2.5x gaps recorded after #969 were
script-level loop/call/for-of cost, and closed with the bytecode work in
#958-#961 (`--no-bytecode` is 1.7-2.0x slower than the default on the lookup
repros), not with further collection changes.

Scaling and size (one script: 3 collections of N keys, two lookup rounds,
half the keys deleted, then full iteration; `jsse` default build):

| N | 25k | 50k | 100k | 200k |
|---|---:|---:|---:|---:|
| jsse | 0.18 s | 0.38 s | 1.03 s | 2.03 s |

Doubling N roughly doubles the time. boa on the same script: 0.81 s, 4.4 s,
37 s (it is superlinear on this shape). 100k Map keys plus 100k WeakMap keys,
set and get: 0.35 s.

## lazy-collections (JetStream)

Not an acceptance criterion; recorded because the issue lists it as
implicated. It makes no Map/Set calls on its hot path: it is generator
`next()` chains (`range`, `flatMap`, `windows`, ...) consumed by `reduce`/
`every` closures. `BAIL construct needs tree-walked frame` kept every
`reduceFn`/`everyFn`/... body (the closures that contain
`(async () => { ... await ... for await ... })()`) on the tree-walker, because
the function-goal poison scan treated an `await` met inside a nested arrow as
the function's own. Only plain functions reach that compile (async and
generator bodies take the state-machine paths), so a `yield`/`await`
expression the scan meets can only belong to a nested arrow, as in the Script
goal. They are no longer poison.

perf-counters build, 4 iterations of the runner's driver:

| | before | after |
|---|---:|---:|
| `compile_bail` | 2 | 0 |
| `body_dispatch_ast` | 27,916 | 0 |
| `ast_units_in_functions` | 32,156,064 | 0 |
| `vm_ops` | 12,789,267 | 20,100,007 |

CPU time for the 4-iteration driver (median of 5): before 6.56 s, after
6.02 s, boa 3.26 s, so lazy-collections is about 1.8-2.1x boa depending on the
run (the quiet-host figure before this change was 2.26x by the runner). What is
left is generator-resume bookkeeping (`reconcile_scope_stack`,
`sync_generator_scope_stack`, `align_generator_for_of_stack`, cloning the
`IteratorState` and the scope/for-of stacks per `next()`, about 15% of sampled
frames) and by-name (`String`) identifier lookups in tree-walked generator
bodies (`find_in` on `str`, about 8%). Neither is collection storage; both
belong to #897.
