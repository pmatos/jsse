# Library Tests: minor-collection cost on the TypeScript runner (#1490)

Issue #1490 (epic #1214, sub-issue of #1418): garbage collection was about half the wall time of every TypeScript test-runner process on `jsse --node`, almost all of it minor collections of about 10 ms each for a 4 MiB nursery.

## Finding

`gc_collect_minor` has a phase timer now (`PERF gc_minor_<phase>_us`, `docs/agents/execution-counters.md`; code in `src/interpreter/perf_counters_gc.rs`). Phases: `roots`, `registry`, `remembered`, `trace`, `sweep`, `settle`; sizes: `gc_minor_registry_total`, `remembered_total`, `nursery_total`, `survivors_total`, `scanned_envs_total` (sums over all minors).

`bundle-load` (6 unit tests = loading the 13 MB `built/local/run.js`), `--features perf-counters`, `release-nolto`, before the change:

| phase | time over 1047 minors |
| --- | ---: |
| `registry` (the `Interpreter::env_registry.retain` in `collect_gc_roots(true)`) | 7.28 s (85% of minor time) |
| `sweep` (free the dead nursery objects) | 0.71 s |
| `trace` | 0.40 s |
| `remembered` | 0.14 s |
| `settle` | 0.06 s |
| `roots` (everything else in the root set) | 0.01 s |

The registry holds a weak reference to every environment a collection has scanned and found clean (`Interpreter::settle_scanned_envs`), so that a minor collection can re-scan the ones written since. It is visited in full at every minor collection: 590 M probes over the run, about 563 K environments each. A probe is a cache miss or two (the `Rc` header and the `clean` mark live in different lines), 12 ns, so a collection of 8,192 nursery objects paid 7 ms for the registry alone. The suspects named in the issue were wrong: the root set other than the registry costs 10 us per minor, the nursery bitmap is sized once (it only grows), and the remembered set is a few hundred entries.

## Fix

1. **The probe no longer writes.** The registry scan reads the `clean` mark through `Weak::as_ptr` and `RefCell::try_borrow_unguarded` instead of `upgrade()` plus `borrow()`, so it neither bumps the strong count nor the borrow flag of 563 K environments per collection (12 ns to 7.4 ns per probe). Only an unclean environment is upgraded.
2. **The nursery grows with the registry** (`GcPacer::retune_nursery`, `src/interpreter/gc/pacer.rs`). After every collection the nursery threshold is `registry_len * 128` bytes, clamped to `[4 MiB, 64 MiB]` of charged bytes (`GC_OBJECT_OVERHEAD` = 512 per object, so up to 128 Ki objects). The per-collection cost that scales with the heap is then amortised over proportionally more allocation, which is the standard remedy when a fixed per-collection cost cannot be removed without a hot-path barrier change (a dirty list needs a branch in every `BindingMap` store; the barrier is a single store on purpose, `docs/agents/architecture-runtime.md`). Heaps under 32 Ki registered environments keep the 4 MiB nursery, so test262 and every small script behave as before.

`GcPacer` and its tests moved to `src/interpreter/gc/pacer.rs` and `pacer_tests.rs` (`gc.rs` is over the 20,000-byte budget and may not grow).

## Results

Counters (`--features perf-counters`, `release-nolto`; deterministic except the time columns, which are the instrumented run's own):

| `bundle-load` | before | probe only | probe + nursery |
| --- | ---: | ---: | ---: |
| minor / major collections | 1047 / 23 | 1047 / 23 | 125 / 23 |
| `gc_ms` (all) | 10,653 | 7,660 | 3,920 |
| `gc_minor_us` | 8,603 K | 5,633 K | 1,669 K |
| `gc_minor_registry_us` | 7,281 K | 4,366 K | 357 K |
| minor time per nursery object (`gc_minor_us / nursery_total`) | 874 ns | 573 ns | 183 ns |

`compiler-first` after the change: 310 minors (4118 before, issue table), `gc_ms` 19,261 (64.6 s in the issue's instrumented run: 70% less), `gc_minor_registry_us` 1.4 s, `gc_minor_sweep_us` 5.4 s (33.8 M nursery objects: freeing is now the largest minor phase).

Wall time and peak RSS (`release-nolto` without counters, LTO off, one process at a time on the shared host, base = origin/main 6930012b):

| case | base | this change | RSS base | RSS this change |
| --- | ---: | ---: | ---: | ---: |
| `bundle-load` | 33.6 s | 23.9 s (-29%) | 1665 MiB | 1723 MiB (+3.5%) |
| `compiler-first` | 103.9 s | 66.5 s (-36%) | 2552 MiB | 2582 MiB (+1.2%) |

Remaining GC time is the 48 major collections of `compiler-first` (about 10 s, heap-proportional by design) and sweeping.

Validation: full test262 99,911/99,911 on both VM modes; `test262-extra/` 1,705/1,705 on a `release-checked` build, plain and `JSSE_GC_STRESS=7`, bytecode and `--no-bytecode`; `bundle-load` under `JSSE_GC_STRESS=20000` exits 0; `cargo test --lib interpreter::gc`.

## Guards

- `src/interpreter/gc/pacer_tests.rs::nursery_grows_with_the_environment_registry_up_to_a_cap`.
- `test262-extra/gc-minor-collection-large-environment-registry.js`: 40,000 old closure environments, stores of young objects into spread cells across several long nursery cycles and a major collection; run in both VM modes and under `JSSE_GC_STRESS`.

## Not done

The registry is still visited in full at each minor collection (7 ns per environment). A dirty list filled when an environment's `clean` mark flips would make that O(written environments) but needs a conditional in the `BindingMap` store path; not attempted without Ir measurements of the interpreter benchmarks (valgrind was not available).
