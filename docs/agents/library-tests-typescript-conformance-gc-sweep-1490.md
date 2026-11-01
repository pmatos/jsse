# Library Tests: minor-collection sweep and nursery size (#1490, stage 3)

Third step of #1490 (epic #1214), after [the registry probe and the dirty list](library-tests-typescript-conformance-gc-1490.md). That step left a minor collection at about 1.1 ms per 4 MiB nursery on `compiler-first`, 83% of it the sweep of dead nursery objects. This step makes the sweep cheaper and removes the nursery growth the first step added, which the dirty list made obsolete.

## Where the sweep time goes

`rdtsc` around the free of each dead object (temporary instrumentation, not committed; `release-nolto` + `perf-counters`, base = origin/main 73f192da), cycles per dead object:

| `compiler-first`, class of the dead object | count | cycles each | share of the drop time |
| --- | ---: | ---: | ---: |
| Array Iterator | 21.9 M | 215 | 64% |
| Arguments | 1.87 M | 556 | 14% |
| closure (`Function`, user code) | 1.27 M | 703 | 12% |
| Array | 4.42 M | 137 | 8% |
| Object | 10.7 M | 36 | 5% |

Findings:

1. Dropping the payload (`set_slot(id, None)`: the object's property storage, element vector, closure record and environment, iterator state box) is 62% of the sweep; the lookups, the side-table checks and the loop itself are the rest. There is no way to make that scale with survivors: every dead object owns heap blocks that `Drop` must give back. The per-chunk bitmap named in the issue would still have to visit each dead slot.
2. A dead object is first touched by the sweep (the trace only visits survivors), so each free starts with cache misses: on the allocation (up to 320 bytes) and on each heap block it owns. The nursery is walked in allocation order, whose blocks are not in address order (the physical blocks come off a LIFO free list), so the hardware prefetcher does not help.
3. 65% of the nursery of `compiler-first` is dead `Array Iterator` objects: every `for (x of array)`, array destructuring and array spread allocates a 264 byte object plus a 192 byte `IteratorState` box (`IteratorState` is 192 bytes because of its generator variants; an array iterator needs 24). Allocating none of them for an unpatched array is the largest remaining lever on this workload, and it is an interpreter change, not a collector one (follow-up below).
4. With the registry scan O(dirty), the nursery growth of the first step (`registry_len * 128` bytes, up to 64 MiB, so up to 131,072 objects of about 450 bytes each, 60 MB) made every minor collection walk a nursery far larger than the cache, so the per-object cost doubled: on `compiler-first` 110 to 156 ns of minor collection per nursery object against 57 to 83 ns with the plain 4 MiB nursery (sweep 88 to 125 against 38 to 48 ns), same survival rate (4.0% against 4.6%) and same number of major collections (48).

## Changes

- `Interpreter::sweep_nursery` (`src/interpreter/gc/minor_sweep.rs`, moved out of `gc_collect_minor`): one slot lookup per object instead of four (`get_cell` instead of a cloned `get`), and two-stage prefetch: the allocation of the object 12 ahead, then (4 ahead, when that has arrived) the heap blocks it owns: a function's `JsFunction` and closure environment, an iterator's or Arguments object's state box, an Array's element buffer (`src/interpreter/object_arena/prefetch.rs`; x86-64 only, a no-op elsewhere). A single stage (allocation only) gave -13 to -18% sweep time; adding the second stage cut the cycles per dead object of every class by 25 to 38% in the instrumented comparison (Array Iterator 215 to 134, Arguments 556 to 402, closure 703 to 477, Array 137 to 93, Object 36 to 27).
- The nursery stays at `GC_NURSERY_THRESHOLD_BYTES` (4 MiB): `GcPacer::retune_nursery`, its two constants and their test are gone. `EnvRegistry::len` is only used by tests and the counters now.
- `ObjectArena`'s unit tests moved to `object_arena/tests.rs` (the file was over the 20,000 byte budget and may not grow).

## Measurements

Host: shared 24 cores, load average 10 to 25 from other agents, one process at a time, `release-nolto`. Memory-bound code is very sensitive to co-tenants here: the same binary varied 1.5x to 2x between runs, so the tables give every run of a series and conclusions rest on pairs run back to back.

`compiler-first` (`--features perf-counters`, `gc_minor_us` and `gc_ms` from the instrumented run; 4118 minors for a 4 MiB nursery, 309 with growth):

| run | minor ns per nursery object | `gc_minor_us` | per minor | `gc_ms` (all) |
| --- | ---: | ---: | ---: | ---: |
| base (growth) | 143, 209, 179 | 4.8, 7.0, 6.0 s | 15 to 23 ms | 14.9, 20.4, 18.1 s |
| sweep prefetch, one stage (growth) | 124, 182, 161 | 4.1, 6.1, 5.4 s | | 14.2, 19.1, 17.8 s |
| two stages (growth), pair 1 / 2 / 3 | 156 / 110 / 130 | 5.2 / 3.7 / 4.3 s | 12 to 17 ms | 17.3 / 13.4 / 15.0 s |
| **two stages, 4 MiB nursery**, pair 1 / 2 / 3 | **83 / 57 / 71** | **3.0 / 2.0 / 2.6 s** | **0.49 to 0.72 ms** | **12.4 / 11.0 / 12.4 s** |

Wall time of the same runs: 55.9 / 48.6 / 52.2 s (4 MiB) against 67.8 / 52.1 / 56.8 s (growth, same pair order); peak RSS 2077 to 2095 MiB against 2114 to 2118 MiB (-1 to -2%).

`bundle-load`: 1048 minors of 4 MiB at 0.75 ms (best, quiet host) to 1.6 ms (loaded) against 126 minors of 6 to 16 ms before, 13% of the nursery surviving (module loading builds long-lived structure, so the trace is a third of the minor: about 0.3 ms). Peak RSS 1578 MiB to 1519 MiB (-3.8%, every run). Wall time cannot be resolved from the host noise (17.2 to 31.8 s for one binary).

Ir, `callgrind` of `--no-default-features` `release-nolto` binaries (valgrind 3.25.1 built from source in a scratch directory), `benchmarks/scripts/` at their shipped sizes. The prefetch loop adds a few instructions per swept object, so the GC-heavy scripts gain a fraction of a percent; interpreter-only scripts are within 0.15%:

| script | Ir base | Ir this change | delta |
| --- | ---: | ---: | ---: |
| `bench_array` | 865.35 M | 864.85 M | -0.06% |
| `bench_closure_create` | 2747.42 M | 2778.92 M | +1.15% |
| `bench_closures` | 6312.66 M | 6362.56 M | +0.79% |
| `bench_fib` | 5650.14 M | 5658.21 M | +0.14% |
| `bench_json` | 351.72 M | 351.61 M | -0.03% |
| `bench_loop` | 3194.52 M | 3194.52 M | -0.00% |
| `bench_object` | 1516.98 M | 1516.69 M | -0.02% |
| `bench_promise_new` | 6941.87 M | 6978.39 M | +0.53% |
| `bench_regex` | 136.93 M | 136.81 M | -0.09% |
| `bench_string` | 357.96 M | 357.86 M | -0.03% |
| synthetic closure/array churn, 300k iterations | 7133.46 M | 7171.53 M | +0.53% |

Wall time of five of these scripts (`release-nolto`, min of 12 runs alternating base and this change): `bench_closures` 0.335 to 0.344 s, `bench_closure_create` 0.144 to 0.146 s, `bench_object` 0.102 to 0.098 s, `bench_fib` 0.242 to 0.246 s, `bench_opmix` 1.427 to 1.490 s: every difference is inside the host's noise (the medians swing by 50%), `bench_closures` (+2.7%) being the one that repeated across two series. These heaps never grow the nursery, so only the prefetch can move them.

Geometric mean over the 11 rows +0.26%; the largest are the closure-creating scripts (+0.5% to +1.2%), whose time is mostly collection.

## Goal of the issue

"A minor collection of a 4 MiB nursery should cost well under a millisecond when the survivors are few": met on `compiler-first` (4.6% survivors): 0.49 to 0.72 ms per minor collection, down from 10 to 12 ms in the issue and 1.1 ms before this step. Not met on `bundle-load` (13% survivors): 0.75 ms at best and about 1.5 ms under load, of which the trace of the survivors is 0.3 ms (about 400 Ir per traced object: `trace_object_fields_in` 220, `enqueue_young` 90) and the sweep 0.4 ms. GC share of a process: `compiler-first` `gc_ms` 17.3 s to 12.4 s of a 68 s to 56 s run, still about a fifth, now mostly the 48 major collections (about 10 s, from the previous step) and the dead-iterator drops.

## What is left

- Not allocating an Array Iterator for `for-of`, array destructuring and spread over an array whose `Symbol.iterator` and `%ArrayIteratorPrototype%.next` are unpatched: 65% of the nursery objects of `compiler-first`, 64% of its drop time, and a saving in the interpreter itself. Observable only through those two hooks, so it needs the guards the existing `iterator_next_cache` path has.
- Shrinking `IteratorState` (192 bytes, because of the generator variants; 84 match sites) so an array iterator's box is a cache line.
- The trace of survivors on survivor-heavy phases (`bundle-load`).

## Validation

Full test262 99,911/99,911 (`release-nolto`, bytecode VM); `test262-extra/` 1,872/1,872 on a `release-checked` build, plain and `JSSE_GC_STRESS=7`, bytecode and `--no-bytecode`; a seeded 3% sample of test262 (5,239 scenarios) under `JSSE_GC_STRESS=7` on the checked build; `cargo test --lib` for `interpreter::gc`, `object_arena` (111 tests). Guards: `test262-extra/gc-minor-sweep-survivors-among-dead-kinds.js` (survivors of six object kinds among dead neighbours across five minor collections and into the old generation), `object_arena/tests.rs::prefetch_hints_accept_live_dead_and_unallocated_ids`. The sweep's observable behaviour is unchanged, so no regression test can fail on the nursery size; the removed pacer test asserted only the growth.
