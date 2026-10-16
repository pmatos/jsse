# #1047: jemalloc as the shipped binary's global allocator

Decision: adopt jemalloc (`tikv-jemallocator` 0.6, default feature `jemalloc`, `#[global_allocator]` in
`src/main.rs` only, `thp:never` embedded via `_rjem_malloc_conf` on Linux). Step 0 (glibc `mallopt` trim tuning) was built and measured first; it is not shipped.

Measured on buildbox3 (AMD EPYC 7501, one pinned core `taskset -c 60`, loadavg 1.5-2.4 from other users),
`jsse` built from `660c5a17` with nightly 1.100.0-nightly, shipped profile (fat LTO, cgu=1), `--locked`.
base = the same tree without the allocator (`--no-default-features`, byte-for-byte the library of `main`),
final = default features. One unrecorded warmup run, then 5 rounds with the variant order rotated each round;
wall = process wall clock including startup.

## Wall time (median seconds, 5 rounds, shipped config: `thp:never`, one arena for glibc)

| benchmark | base | jemalloc | ratio | maxrss base -> jemalloc |
|---|---:|---:|---:|---:|
| bigint-noble-ed25519 | 2.238 | 2.123 | 0.949 | 42 -> 42 MB |
| bigint-noble-bls12-381 | 23.685 | 22.790 | 0.962 | 37 -> 35 MB |
| proxy-mobx | 0.322 | 0.303 | 0.942 | 36 -> 35 MB |
| proxy-vue | 0.571 | 0.528 | 0.926 | 36 -> 34 MB |
| async-fs | 2.989 | 2.765 | 0.925 | 30 -> 27 MB |
| splay | 3.378 | 3.095 | 0.916 | 526 -> 537 MB (+2.1%) |
| richards | 3.727 | 3.730 | 1.001 | 20 -> 17 MB |
| delta-blue | 2.438 | 2.439 | 1.001 | 24 -> 21 MB |
| hash-map | 5.859 | 5.948 | 1.015 | 68 -> 66 MB |
| fib | 1.646 | 1.623 | 0.985 | 18 -> 16 MB |

Geomean of the six allocation-heavy rows: 0.937 (-6.3%); all ten:
0.962 (-3.8%). hash-map's +1.5% is inside its 0.8-1.1% spread plus layout noise (richards, delta-blue and
fib, the same interpreter loop, are flat or faster). Run-to-run spread (max-min)/median was 0.8-5%.

## Transparent huge pages decide how much you get

buildbox3 (and this laptop) run with THP `enabled=always`; Ubuntu/GitHub runners use `madvise`. jemalloc's
default (`thp:default`) never calls `madvise`, so on an `always` host the kernel backs every 2 MiB-aligned
extent with a zeroed huge page:

| config (buildbox3, THP=always) | six-row geomean | splay | test262 10% sample, 8 cores |
|---|---:|---:|---:|
| glibc (base) | 1.000 | 3.163 s | 41.5 s |
| jemalloc `thp:default` | 0.904 | 0.817x, 15-20k minor faults | 48.7 s (+17%) |
| jemalloc `thp:never` (shipped) | 0.937 | 0.916x, 183-228k faults | 42.8 s (+3%) |

`thp:default` is 3.3 points better on the long benchmarks (a 550 MB heap takes 15k huge-page faults instead of
140k+ small ones) and it pays for that in every short process: each `jsse` run zeroes huge pages it hardly
touches, which costs 17% on a test262 run (one process per scenario). The shipped `thp:never` behaves the same
on `always` and `madvise` hosts, so CI (Ubuntu, `madvise`) sees what these numbers show. The test262 wall
times for base vs the shipped binary alternated at 52.7/54.9, 52.7/52.9 and 43.4/44.2 s under external load 35-75
(+0.4% to +4%); the quiet-host pair before the final build was 41.5 vs 42.8 s.

The splay minor-fault drop that the issue attributed to jemalloc is therefore mostly THP: with `thp:never`
splay still takes 183-228k faults (floor for 537 MB of 4 KiB pages is 137k). What jemalloc gives without THP
is less kernel time (sys 0.43-0.51 s -> 0.30-0.33 s on splay) and fewer user-space instructions.

## glibc arenas under RLIMIT_AS

The first jemalloc build regressed 3-10 scenarios per run of the Atomics `notify`/`waitAsync` agent tests
under the runner's 512 MiB `RLIMIT_AS` (`memory allocation of N bytes failed`; 0 of 32 failures on glibc).
Each agent is a thread, and with jemalloc serving Rust allocations glibc still gives every thread its own arena
that reserves 64 MiB of address space for libc's own small allocations, on top of jemalloc's extents. One
`mallopt(M_ARENA_MAX, 1)` call at startup (Linux/glibc, jemalloc builds only) removes it: 0 of 32 failures,
and the agent test now passes down to `ulimit -v 400000`, where glibc needs 500000.

## Step 0: glibc `mallopt(M_TRIM_THRESHOLD, 1 GiB)` + `mallopt(M_TOP_PAD, 64 MiB)` at startup

Built as a temporary feature and measured in the same rounds (5 rounds, same host, same sha):

| benchmark | base | mallopt | ratio | jemalloc ratio |
|---|---:|---:|---:|---:|
| bigint-noble-ed25519 | 2.140 | 2.108 | 0.985 | 0.923 |
| proxy-mobx | 0.309 | 0.277 | 0.896 | 0.883 |
| proxy-vue | 0.560 | 0.531 | 0.949 | 0.917 |
| async-fs | 2.870 | 2.785 | 0.970 | 0.912 |
| splay | 3.150 | 2.718 | 0.863 | 0.824 |
| bigint-noble-bls12-381 | 23.683 | 23.351 | 0.986 | 0.926 |
| fib | 1.649 | 1.775 | 1.076 | 0.987 |

Geomean of the six: 0.940 (THP-default jemalloc 0.897 in the same rounds). It clears the 5% bar on the six
rows, but it is not shipped:

- fib came out 7.6% slower in that binary (9 rounds, 1.775 vs 1.649 s) and richards/hash-map got nothing.
  The regression is code layout, not allocation: callgrind `bench_fib.js` is 8,503,128,501 Ir (base) vs
  8,503,717,485 (mallopt) vs 8,476,964,995 (jemalloc), and the same `GLIBC_TUNABLES` values on the unmodified
  binary measure 1.62-1.70 s. A one-function change in `main.rs` moved the interpreter loop's alignment in a
  fat-LTO binary; chasing that is not a fix.
- Against the shipped `thp:never` jemalloc it is a wash on the geomean (6.0% vs 6.3%, different rounds) but
  loses on ed25519 (0.985 vs 0.949), async-fs (0.970 vs 0.925), vue, and bls12-381, and wins only on splay
  (0.863 vs 0.916) and mobx; it also leaves jemalloc's lower user time on the table. Against jemalloc with
  THP (what the issue's numbers measured) it recovers about 60% (6.0% vs 10.3% in the same rounds).
- It is glibc-only, turns off glibc's dynamic mmap threshold, and never returns top-of-heap memory to the OS.

## Startup and size

- `jsse -e 1`: 13.04 ms -> 13.88 ms per spawn (+0.8 ms, 10 rotated rounds of 30 spawns, medians, host under
  load 37-58 so the absolute numbers are noisy; minima 12.33 vs 12.53 ms).
- `jsse -e 1` maxrss: 15.8 MB -> 18.3 MB (27 MB with THP).
- Binary: 31,391,592 -> 31,960,752 bytes (+0.57 MB).
- Full test262 (99,911 scenarios, `-j 22`): 100% on the default and `--no-bytecode` runs; test262-extra 1399/1399
  including `JSSE_GC_STRESS=7` in both modes; `tests/` custom 19/19.

## Notes

- Not resolved on Windows or wasm (`[target.'cfg(not(any(windows, target_family = "wasm")))'.dependencies]`);
  the allocator attribute is also off under miri. The release job builds only `x86_64-unknown-linux-gnu`.
- The library, unit tests, miri and the fuzz targets (`fuzz/Cargo.toml` sets `default-features = false`) keep the
  system allocator, so CodSpeed (`cargo codspeed`, lib benches) will not show this change.
- `--no-default-features` gives the pre-change allocator for valgrind heap tools and for comparison with older
  checkpoints.
