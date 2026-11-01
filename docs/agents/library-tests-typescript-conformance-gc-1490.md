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
2. **The nursery grows with the registry** (removed again once the dirty list made the registry scan O(dirty): [stage 3](library-tests-typescript-conformance-gc-sweep-1490.md)) (`GcPacer::retune_nursery`, `src/interpreter/gc/pacer.rs`). After every collection the nursery threshold is `registry_len * 128` bytes, clamped to `[4 MiB, 64 MiB]` of charged bytes (`GC_OBJECT_OVERHEAD` = 512 per object, so up to 128 Ki objects). The per-collection cost that scales with the heap is then amortised over proportionally more allocation, which is the standard remedy when a fixed per-collection cost cannot be removed without a hot-path barrier change (a dirty list needs a branch in every `BindingMap` store; the barrier is a single store on purpose, `docs/agents/architecture-runtime.md`). Heaps under 32 Ki registered environments keep the 4 MiB nursery, so test262 and every small script behave as before.

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

## Dirty list (second step)

The registry is no longer visited in full. `gc::env_registry` (`src/interpreter/gc/env_registry.rs`) is a per-thread slab of weak environment references; each registered `BindingMap` carries its slot (`binding_map/slot.rs`). A registered environment is either clean or its slot is on the dirty list, which the write barrier fills when it flips a clean mark off (`BindingMap::dirty`: `if clean { clean = false; note_dirty(slot) }`, the push in a cold out-of-line function) and the collector fills for an environment a scan left holding a nursery object (`BindingMap::rearm`, in `settle_scanned_envs`). A minor collection drains the list (upgrading the entries of its own interpreter) instead of walking the registry, so its cost is the number of environments written, not registered. `Drop for BindingMap` frees the slot at once, so the registry size is exact (the nursery retune it fed, `registry_len * 128`, was removed in [stage 3](library-tests-typescript-conformance-gc-sweep-1490.md)); a major collection settles every environment it reaches clean and then keeps queued only the live, still-not-clean entries of its interpreter (so the queue stays bounded when minors are suppressed, and an environment the walk missed is not forgotten), and frees slots whose release could not run. Nothing replaces or moves a `BindingMap` inside a live environment (the #1420 trim writes through `entry_at_mut`), so no path leaves a registered environment not clean without a queue entry. Entries carry the owning interpreter so two interpreters on one thread never scan each other's environments; a stale entry (clean by now, or its slot reused) only costs a scan.

### Measurements

Host: shared 24 cores at load 10 to 15, `release-nolto`, base = origin/main 1502f2c2. Valgrind 3.25.1 was built from source in a scratch directory (not on the host); Ir below is `callgrind` of `--no-default-features` binaries, 11 scripts of `benchmarks/scripts/` (reduced iteration counts) and four binding-store loops (60,000 to 400,000 iterations).

| script | Ir base | Ir this change | delta |
| --- | ---: | ---: | ---: |
| `bench_array` | 262.53 M | 263.61 M | +0.41% |
| `bench_closure_create` | 276.00 M | 277.12 M | +0.41% |
| `bench_closures` | 764.07 M | 765.54 M | +0.19% |
| `bench_fib` | 318.82 M | 321.59 M | +0.87% |
| `bench_json` | 74.71 M | 74.71 M | 0.00% |
| `bench_loop` | 483.82 M | 481.86 M | -0.40% |
| `bench_object` | 461.45 M | 461.35 M | -0.02% |
| `bench_opmix` | 1244.97 M | 1243.06 M | -0.15% |
| `bench_promise_new` | 696.84 M | 697.17 M | +0.05% |
| `bench_regex` | 31.96 M | 31.95 M | -0.03% |
| `bench_string` | 76.81 M | 76.59 M | -0.29% |
| store: closure counter | 242.99 M | 243.11 M | +0.05% |
| store: `let` locals in a loop | 74.18 M | 73.76 M | -0.57% |
| store: closure over object | 245.11 M | 245.77 M | +0.27% |
| store: global `var` | 295.4 M to 302.6 M | 294.10 M | noise (hash-randomised, 2.4% between two base runs) |

Barrier alone (branch plus empty cold call, no registry): -0.8% to +0.2% on nine of them, so the test of the clean mark costs nothing measurable; the `bench_fib` +0.87% is `Environment::bind_this_for_call` no longer being inlined (it holds a `BindingMap` write and the cold call). Forcing `#[inline]` on it moved `bench_loop` +1.4% and `bench_array` +1.1% instead, so it was left alone. Geometric mean over the 14 rows but the hash-noisy one: +0.06%.

The same 15 scripts on the shipped profile (`--release`, fat LTO, `--no-default-features`; base binary built from 1502f2c2, head from the same tree plus unrelated node-host commits): -1.65% (`store: closure counter`) to +0.87% (`bench_fib` again), `bench_array` +0.36%, `bench_closure_create` +0.50%, `bench_closures` +0.37%, `bench_opmix` +0.36%, `bench_loop` +0.06%, the rest within 0.6%; geometric mean +0.14%. The 1-3% regression the old single-store barrier note warns about does not appear.

Counters (`--features perf-counters`, deterministic except the time columns; minor and major counts and `gc_minor_nursery_total` identical to base):

| | `bundle-load` base | this change | `compiler-first` base | this change |
| --- | ---: | ---: | ---: | ---: |
| `gc_minor_registry_us` | 357 to 400 K | 53 to 56 K | 1,066 K | 37 K |
| `gc_minor_settle_us` | 72 to 94 K | 111 to 124 K | 55 K | 79 K |
| `gc_minor_us` (all minors) | 1.72 to 2.03 M | 1.56 to 1.91 M | 5.59 M | 4.38 M |
| `gc_ms` (all collections) | 4.1 to 4.7 s | 4.3 to 5.0 s | 15.1 s | 14.1 s |

The remaining registry time is the walk of the dirty environments, which the old scan also paid. Settle costs more (one slab insert per newly scanned environment instead of a `Vec` push).

Wall time and peak RSS, `release-nolto` (jemalloc), alternating runs, one process at a time, CPU = user + system: `compiler-first` wall 52.3 s min / 52.7 s median (base) against 51.4 s / 52.0 s, CPU the same within 0.1 s, RSS 2118 MiB both: a 1.0 to 1.7% gain. `bundle-load`: 17.7 s min / 19.6 s median against 18.0 s / 18.6 s, RSS 1579 MiB against 1577 MiB: inside the host's noise (17.8 to 24.3 s for the same binary), the 1.5% the counters predict cannot be resolved there.

### Goal of the issue

Not met. A minor collection of the 4 MiB nursery (8,192 objects) still costs about 1.1 ms on `compiler-first` (`gc_minor_us / gc_minor_nursery_total` = 130 ns per object, 4.38 s over 33.5 M objects), of which sweep is 83% (`gc_minor_sweep_us` 3.63 s, 108 ns per object) and trace 9%. The registry share fell from 19% to under 1%. The remaining lever is the sweep of the dead nursery objects (freeing each one's property storage and arena slot), which is per object and unrelated to the heap size; it is the next target if minor collections matter again. The nursery growth of the first step stayed at this stage (it also lowered the number of survivors, 1.3 M of 33.5 M objects); [stage 3](library-tests-typescript-conformance-gc-sweep-1490.md) measured dropping it and removed it.

### Validation

Full test262 99,911/99,911 (`release-nolto`); `test262-extra/` 1,798/1,798 on a `release-checked` build, plain and `JSSE_GC_STRESS=7`, bytecode and `--no-bytecode`; a seeded 3% test262 sample (5,225 scenarios) under `JSSE_GC_STRESS=7` on the checked build; `cargo test --lib` 1,338 tests on `release-checked`, debug assertions on (nine new ones in `gc/env_registry_tests.rs`). Guards: `test262-extra/gc-minor-collection-dirty-environment-list.js` (environments are the only references to young objects across three minor collections without further stores, then dead environments and slot reuse; it fails with a collector that marks a still-young environment clean), `gc/env_registry_tests.rs`.
