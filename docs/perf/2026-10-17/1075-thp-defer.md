# #1075: huge pages for large heaps without taxing short processes

Outcome: the shipped binary no longer embeds jemalloc's `thp:never`. It disables transparent huge pages for the whole
process from a constructor that runs before the allocator's first allocation (`prctl(PR_SET_THP_DISABLE, 1)`), and the
collector lifts that after a major collection finds the process resident set at 20 MiB or more (`src/thp.rs`,
`thp::after_major_collection`, one atomic load while nothing is deferred). Memory faulted before that stays on 4 KiB
pages; memory faulted after it may use huge pages. `main` reads
`/sys/kernel/mm/transparent_hugepage/enabled` once and lifts the early disable unless it says `[always]`, so a `madvise`
or `never` host runs exactly as before (jemalloc's default `thp:default` never calls `madvise`).

The acceptance bar (six-row geomean at least 3 points better than #1047 with test262 within 2%) is **not** met on the
six-row geomean: large-heap rows gain, small-heap rows do not. test262 is not taxed. See "Why not 3 points".

## Why prctl, not the issue's other candidates

- `hpa:true`: serves only allocations up to `hpa_slab_max_alloc` (64 KiB) from hugepage-aligned slabs. The heap that
  matters here is a few big blocks and many mid-size vectors, so there is little for it to hugify; not built or measured.
- `thp:never` plus `madvise(MADV_HUGEPAGE)` on heap blocks: `thp:never` marks every extent `VM_NOHUGEPAGE` and the
  extents are jemalloc's, not the engine's. A process-wide switch that the collector flips needs neither.
- `prctl(PR_SET_THP_DISABLE)` for the first N MB: this is the change, with "N" measured as resident set at a major GC
  because the collector is the one place that runs often enough to notice growth and rarely enough to be free.

The constructor matters: with `prctl` in `main`, the allocator's own bookkeeping (first extents, faulted before `main`)
already took zeroed huge pages (a harness-sized script: maxrss 24.4 MB instead of 19.9 MB) and test262 was +5% instead of -2%.

## Measured (buildbox3, AMD EPYC 7501, THP `enabled=always`, `defrag=madvise`)

Shipped profile (fat LTO, cgu=1, nightly, `--locked`). base = `origin/main` `e22d6a91` (sha256 `4d21e856...`), final =
this change (`0bd6301d...`). Both binaries were copied into `/dev/shm` by a process pinned to the measuring core before
timing: the page cache of a freshly linked binary sits on whichever NUMA node the linker ran on, and the same binary
measured 14.4 ms in `target/` and 11.4 ms from `/dev/shm` for `jsse -e 1` (a first test262 comparison from `target/`
showed a bogus 25% gap between two builds with identical behaviour). One pinned core (`taskset -c 97`), variant order
rotated every round, medians, loadavg 70 from other users but cpu/wall >= 0.995 in every row.

### Six rows, 7 rounds, base vs final

| row | base | final | ratio | minor faults base -> final |
|---|---:|---:|---:|---:|
| splay | 2.761 | 2.445 | 0.886 | 183,811 -> 18,071 |
| bigint-noble-ed25519 | 1.821 | 1.836 | 1.008 | 11,334 -> 7,952 |
| proxy-mobx | 0.241 | 0.243 | 1.008 | 6,393 -> 4,679 |
| proxy-vue | 0.408 | 0.412 | 1.010 | 6,455 -> 5,196 |
| async-fs | 1.921 | 1.931 | 1.005 | 7,385 -> 7,406 |
| bigint-noble-bls12-381 | 18.311 | 18.618 | 1.017 | 20,251 -> 15,150 |

Geomean 0.988 (-1.2%); splay maxrss 548 -> 582 MB (+6%), the other rows within 1 MB. The five small rows are slower
by 0.5-1.7% in this base-vs-final comparison, but that is binary layout, not the allocator policy: comparing the same
binary with the switch back on (`JSSE_THP_RESIDENT_MB` was a temporary experiment knob) against the same binary with
it off (threshold 100,000 MB), 6 rounds:

| row | off | on (20 MB) | ratio |
|---|---:|---:|---:|
| splay | 2.772 | 2.458 | 0.887 |
| bigint-noble-ed25519 | 1.840 | 1.824 | 0.991 |
| proxy-mobx | 0.244 | 0.242 | 0.992 |
| proxy-vue | 0.416 | 0.418 | 1.005 |
| async-fs | 1.961 | 1.964 | 1.002 |
| bigint-noble-bls12-381 | 18.462 | 18.515 | 1.003 |

Geomean 0.979 (-2.1%). Thresholds 16, 20, 24 and 32 MB gave splay 0.88-0.89 and small rows within +-1.5% of each
other (run-to-run spread of the small rows is 0.5-1%); 20 MB was kept because it is a few MB above a bare process and
still catches every row whose fault count falls.

### test262 (10% sample, `--seed 7`, 7 rounds, 7 cores pinned, base vs final)

| | base | final |
|---|---:|---:|
| wall (median) | 47.4 s (47.3-47.7) | 46.3 s (46.0-46.3) |

-2.3%, not +17% (#1047's `thp:default`) or +5% (the first version, without the constructor). Variants of the same
binary with the switch at 16/20/24 MB and off were indistinguishable (46.1-46.2 s, 5 rounds): a short script never runs
a major collection, so the threshold is not what protects it, the early disable is. `jsse -e 1`: 11.3 -> 10.8 ms
(hyperfine, 400 runs, repeated), maxrss 18.2 -> 16.7 MB; the `madvise(MADV_NOHUGEPAGE)` calls jemalloc made per
process (and the VMA splits they cause) are gone.

## Why not 3 points

Same-binary ceiling, `thp:default` (huge pages from the first byte) vs `thp:never`, 5 rounds: six-row geomean 0.963
(-3.7%): splay 0.876, ed25519 0.982, mobx 0.983, vue 0.973, async-fs 0.986, bls12-381 0.985. Splay supplies 2.1 of the 3.7
points; the other five rows gain 1.4-2.7% each, from huge pages covering memory faulted in the first 20-40 MB (parse,
setup, first iteration). A deferral cannot have that: the memory is faulted before the heap "has grown", and turning
huge pages on earlier is exactly the zeroed-huge-page-per-fresh-extent cost that taxes a test262 process. What
deferral keeps is the long-heap part (splay 0.887, fault count -90%). Reaching 3 points would need the small-heap
rows' early memory on huge pages without charging short processes; no signal separates them at startup.

## Notes

- Linux only (`prctl`, `/proc/self/statm`, sysfs); other targets compile the module to no-ops. The constructor and the
  call are only in builds with the `jemalloc` feature (the library, unit tests, miri and fuzz targets are untouched).
- A kernel or sandbox that rejects `PR_SET_THP_DISABLE` leaves jemalloc's default behaviour, which is the #1047
  `thp:default` cost on a THP=always host. Docker's default seccomp profile allows `prctl`.
- `PR_SET_THP_DISABLE` is inherited across `fork`/`exec`; the engine spawns no child processes.
- Not measured: a THP `madvise` or `never` host (none available without root). By construction the early disable is
  lifted before any user code runs and jemalloc issues no `madvise`, so allocation behaviour matches the old
  `thp:never` there; the `jsse -e 1` syscall trace differs only by the dropped `MADV_NOHUGEPAGE` calls.
- Tests: `thp::tests` (unit, toggles the process flag), `tests/thp_defer.rs` (a small process reports
  `THP_enabled: 0`, a script that builds a few hundred thousand objects reports `1` after its heap grows; both skip on
  a host that is not THP=always).
