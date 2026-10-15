# #1047: jemalloc as the shipped binary's global allocator

Decision: adopt jemalloc (`tikv-jemallocator` 0.6, default feature `jemalloc`, `#[global_allocator]` in
`src/main.rs` only). Step 0 (glibc `mallopt` trim tuning) was built and measured first; it is not shipped.

Measured on buildbox3 (AMD EPYC 7501, one pinned core `taskset -c 60`, loadavg 1.5-2.4 from other users),
`jsse` built from `660c5a17` with nightly 1.100.0-nightly, shipped profile (fat LTO, cgu=1), `--locked`.
base = the same tree without the allocator (`--no-default-features`, byte-for-byte the library of `main`),
final = default features. One unrecorded warmup run, then 5 rounds with the variant order rotated each round;
wall = process wall clock including startup.

## Wall time (median seconds, 5 rounds)

| benchmark | base | jemalloc | ratio | maxrss base -> jemalloc |
|---|---:|---:|---:|---:|
| bigint-noble-ed25519 | 2.244 | 2.080 | 0.927 | 43 -> 47 MB |
| bigint-noble-bls12-381 | 22.452 | 21.159 | 0.942 | 36 -> 44 MB |
| proxy-mobx | 0.328 | 0.295 | 0.902 | 33 -> 43 MB |
| proxy-vue | 0.562 | 0.519 | 0.922 | 36 -> 41 MB |
| async-fs | 2.987 | 2.740 | 0.917 | 27 -> 35 MB |
| splay | 3.163 | 2.584 | 0.817 | 538 -> 564 MB (+4.8%) |
| richards | 3.553 | 3.523 | 0.992 | 19 -> 27 MB |
| delta-blue | 2.291 | 2.247 | 0.981 | 21 -> 31 MB |
| hash-map | 5.511 | 5.492 | 0.997 | 66 -> 76 MB |
| fib | 1.513 | 1.505 | 0.995 | 15 -> 25 MB |

Geomean of the six allocation-heavy rows: 0.904 (-9.6%); all ten: 0.938 (-6.2%). No row regresses. Run-to-run
spread (max-min)/median was 0.7-6%. An earlier build without the `M_ARENA_MAX` call below measured 0.909
(six) and the same pattern per row.

## Where it comes from (splay, `/usr/bin/time`)

| variant | wall | user | sys | minor faults | maxrss |
|---|---:|---:|---:|---:|---:|
| glibc | 3.42 / 3.15 | 2.93 / 2.75 | 0.48 / 0.39 | 252,822 / 191,686 | 550 MB |
| jemalloc | 2.59 / 2.60 | 2.49 / 2.49 | 0.10 / 0.10 | 20,691 / 16,456 | 577 MB |

glibc trims the engine thread's heap after each collection and refaults it for the next allocation burst:
kernel time and page faults that callgrind does not model. ed25519 minor faults 13.7k -> 6.5k, proxy-mobx
6.4k -> 3.9k, async-fs 7.1k -> 5.3k.

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

Geomean of the six: 0.940 (jemalloc 0.897 in the same rounds). It clears the 5% bar on the six rows, but it
is not shipped:

- fib came out 7.6% slower in that binary (9 rounds, 1.775 vs 1.649 s) and richards/hash-map got nothing.
  The regression is code layout, not allocation: callgrind `bench_fib.js` is 8,503,128,501 Ir (base) vs
  8,503,717,485 (mallopt) vs 8,476,964,995 (jemalloc), and the same `GLIBC_TUNABLES` values on the unmodified
  binary measure 1.62-1.70 s. A one-function change in `main.rs` moved the interpreter loop's alignment in a
  fat-LTO binary; chasing that is not a fix.
- It recovers only about 60% of jemalloc's geomean (6.0% vs 10.3% on the same rounds) and ed25519, async-fs
  and bls12-381 keep most of their cost.
- It is glibc-only, turns off glibc's dynamic mmap threshold, and never returns top-of-heap memory to the OS.

## Startup and size

- `jsse -e 1`: 13.10 ms -> 13.89 ms per spawn (+0.8 ms, 10 rotated rounds of 30 spawns, medians).
- `jsse -e 1` maxrss: 16 MB -> 27 MB.
- Binary: 31,391,592 -> 31,960,688 bytes (+0.57 MB).
- Full test262 (99,911 scenarios, `-j 22`): 100% on the default and `--no-bytecode` runs; test262-extra 1399/1399
  including `JSSE_GC_STRESS=7` in both modes; `tests/` custom 19/19.

## Notes

- Not resolved on Windows or wasm (`[target.'cfg(not(any(windows, target_family = "wasm")))'.dependencies]`);
  the allocator attribute is also off under miri. The release job builds only `x86_64-unknown-linux-gnu`.
- The library, unit tests, miri and the fuzz targets (`fuzz/Cargo.toml` sets `default-features = false`) keep the
  system allocator, so CodSpeed (`cargo codspeed`, lib benches) will not show this change.
- `--no-default-features` gives the pre-change allocator for valgrind heap tools and for comparison with older
  checkpoints.
