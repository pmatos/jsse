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

- `Interpreter::sweep_nursery` (`src/interpreter/gc/minor_sweep.rs`, moved out of `gc_collect_minor`): one slot lookup per object instead of four (`get_cell` instead of a cloned `get`), and two-stage prefetch: the allocation of the object 12 ahead, then (4 ahead, when that has arrived) the heap blocks it owns: a function's `JsFunction`, an iterator's or Arguments object's state box, an Array's element buffer (`src/interpreter/object_arena/prefetch.rs`; x86-64 only, empty elsewhere). A single stage (allocation only) gave -13 to -18% sweep time; adding the second stage cut the cycles per dead object of every class by 25 to 38% in the instrumented comparison (Array Iterator 215 to 134, Arguments 556 to 402, closure 703 to 477, Array 137 to 93, Object 36 to 27; that build also prefetched the closure environment of a function, which review found stalls on the line just requested, so the shipped stage 2 does not).
- The lookahead runs only once the heap holds 65,536 live objects (`PREFETCH_MIN_LIVE_OBJECTS`). Below that the blocks of a nursery are recycled from a small address range and stay cached, and the lookahead was pure cost there (first version, unconditional: +0.5% to +1.2% Ir and a repeatable +2.7% wall on `bench_closures`).
- The nursery stays at `GC_NURSERY_THRESHOLD_BYTES` (4 MiB): `GcPacer::retune_nursery`, its two constants and their test are gone. `EnvRegistry::len` is only used by tests and the counters now.
- `ObjectArena`'s unit tests moved to `object_arena/tests.rs` (the file was over the 20,000 byte budget and may not grow).

## Measurements

Host: shared 24 cores, `release-nolto`, one process at a time, runs of two binaries alternating. Memory-bound code is very sensitive to co-tenants here: while other agents were running the same binary varied 1.5x to 2x between runs (the intermediate rows below), and in a quiet window it repeated to 1%. The last rows of each table are the final code in such a window.

`compiler-first` (`--features perf-counters`, `gc_minor_us` and `gc_ms` from the instrumented run; 4118 minors for a 4 MiB nursery, 309 with growth):

| run | minor ns per nursery object | `gc_minor_us` | per minor | `gc_ms` (all) |
| --- | ---: | ---: | ---: | ---: |
| base (growth) | 143, 209, 179 | 4.8, 7.0, 6.0 s | 15 to 23 ms | 14.9, 20.4, 18.1 s |
| sweep prefetch, one stage (growth) | 124, 182, 161 | 4.1, 6.1, 5.4 s | | 14.2, 19.1, 17.8 s |
| two stages (growth), pair 1 / 2 / 3 | 156 / 110 / 130 | 5.2 / 3.7 / 4.3 s | 12 to 17 ms | 17.3 / 13.4 / 15.0 s |
| two stages, 4 MiB nursery, loaded host, pair 1 / 2 / 3 | 83 / 57 / 71 | 3.0 / 2.0 / 2.6 s | 0.49 to 0.72 ms | 12.4 / 11.0 / 12.4 s |
| quiet window: base (growth), 3 runs | 127, 129, 129 | 4.25, 4.30, 4.30 s | 13.8 to 14.0 ms | 13.9, 13.9, 13.8 s |
| **quiet window: shipped code (4 MiB nursery, gated two-stage prefetch)**, 3 runs | **66, 55, 56** | **2.39, 1.99, 2.02 s** | **0.48 to 0.58 ms** | **11.5, 10.8, 10.9 s** |

Wall time of the quiet-window runs (instrumented build): 52.3, 52.1, 51.9 s for the base against 48.1, 45.9, 46.4 s (-8 to -12%); peak RSS 2115 to 2117 MiB against 2077 to 2079 MiB (-1.8%). The loaded-host pairs gave 55.9 / 48.6 / 52.2 s against 67.8 / 52.1 / 56.8 s for growth plus prefetch.

`bundle-load` (quiet window, 3 runs of each, instrumented build): base 126 minors, 151 to 152 ns per nursery object, `gc_minor_us` 1.39 to 1.40 s, `gc_ms` 3.75 s, wall 17.4 to 17.7 s, RSS 1578 MiB; shipped code 1048 minors, 77 to 78 ns per nursery object, `gc_minor_us` 0.76 to 0.77 s (0.73 ms per minor of about 9,400 nursery objects, 0.63 ms scaled to 8,192), `gc_ms` 2.96 s, wall 15.9 to 16.0 s (-9%), RSS 1518 to 1520 MiB (-3.8%). 13% of its nursery survives (module loading builds long-lived structure), so the trace is a third of a minor (0.24 ms).

Ir, `callgrind` of `--no-default-features` `release-nolto` binaries (valgrind 3.25.1 built from source in a scratch directory), `benchmarks/scripts/` at their shipped sizes, base against the shipped change. Their heaps are below the prefetch threshold, so only the `live_count` test is added; the unconditional first version cost +1.15% on `bench_closure_create`, +0.79% on `bench_closures`, +0.53% on `bench_promise_new` and on the synthetic churn:

| script | Ir base | Ir this change | delta |
| --- | ---: | ---: | ---: |
| `bench_closure_create` | 2747.42 M | 2747.70 M | +0.01% |
| `bench_closures` | 6312.66 M | 6313.60 M | +0.01% |
| `bench_promise_new` | 6941.87 M | 6939.06 M | -0.04% |
| `bench_fib` | 5650.14 M | 5658.21 M | +0.14% (no collection to speak of: code layout) |
| `bench_array` | 865.35 M | 865.65 M | +0.03% |
| `bench_object` | 1516.98 M | 1516.73 M | -0.02% |
| synthetic closure/array churn, 300k iterations | 7133.46 M | 7131.01 M | -0.03% |

The micro-suite wall time was measured with the unconditional version only (min of 12 alternating runs: `bench_closures` 0.335 to 0.344 s, `bench_object` 0.102 to 0.098 s, `bench_fib` 0.242 to 0.246 s, medians swinging by 50% with the host); the shipped version runs the same instructions as the base there.

## Goal of the issue

"A minor collection of a 4 MiB nursery should cost well under a millisecond when the survivors are few": met. `compiler-first` (4.6% survivors): 0.48 to 0.58 ms per minor collection, `bundle-load` (13% survivors, the harder case): 0.73 ms; the issue measured 9.7 and 12.3 ms, the previous step left 1.1 ms. Per nursery object a minor collection costs 55 to 78 ns, from 127 to 152 ns. GC is still about a quarter of a `compiler-first` process (`gc_ms` 10.8 to 11.5 s of 46 to 48 s, instrumented), no longer minor collections (2.0 to 2.4 s) but the 48 major collections (about 9 s) that the heap-proportional budget schedules, plus the dead-iterator drops counted in the minors.

## What is left

- Not allocating an Array Iterator for `for-of`, array destructuring and spread over an array whose `Symbol.iterator` and `%ArrayIteratorPrototype%.next` are unpatched: 65% of the nursery objects of `compiler-first`, 64% of its drop time, and a saving in the interpreter itself. Observable only through those two hooks, so it needs the guards the existing `iterator_next_cache` path has.
- Shrinking `IteratorState` (192 bytes, because of the generator variants; 84 match sites) so an array iterator's box is a cache line.
- The trace of survivors on survivor-heavy phases (`bundle-load`: 0.24 ms of a minor, about 400 Ir per traced object, `trace_object_fields_in` 220 and `enqueue_young` 90).
- The 48 major collections of `compiler-first`.

## Validation

Full test262 99,911/99,911 on both VM modes (`release-nolto`); `test262-extra/` 1,872/1,872 on a `release-checked` build, plain and `JSSE_GC_STRESS=7`, bytecode and `--no-bytecode`; a seeded 3% sample of test262 (5,239 scenarios) under `JSSE_GC_STRESS=7` on the checked build; `cargo test --lib` for `interpreter::gc` and `object_arena` (97 tests). Guards: `test262-extra/gc-minor-sweep-survivors-among-dead-kinds.js` (a 70,000-object ballast so the sweep prefetches; survivors of six object kinds among dead neighbours across seven minor collections and into the old generation), `object_arena/tests.rs::prefetch_hints_follow_payload_kinds_and_leave_objects_intact` (the hints on a function, an Array and an iterator, and on dead and unallocated ids). A prefetch has no observable effect by design and the nursery size is not observable from script, so neither guard can fail on the optimisation itself; they fail if the sweep frees or ages a survivor wrongly, or the hints fault. The removed pacer test asserted only the growth.
