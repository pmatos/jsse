# #935: fat LTO + codegen-units=1 for the release profile

Decision: adopt. Measured on buildbox3 (AMD EPYC 7501, loadavg 1-3 from the benchmark's own process, jsse
`7d583341` built with nightly 1.100.0-nightly 495c385d0), base = default release profile, LTO = `lto = "fat"`,
`codegen-units = 1`. Binaries snapshotted before any run; every comparison is interleaved (base/LTO alternate,
LTO-first on even rounds). Outputs matched between the two binaries in every micro benchmark.

## Build and size cost (buildbox3, cold = nothing cached)

| step | base | LTO |
|---|---:|---:|
| `cargo build --release`, all 61 cores, cold | 2m17s (544 s user) | 4m51s (470 s user) |
| `cargo build --release -j4`, crate only (deps cached) | 71 s | 194-236 s |
| `cargo test --release --no-run -j4`, crate only | 149 s | 313 s (every test binary is LTO-relinked) |
| `cargo test --profile release-nolto --no-run -j4`, crate only | n/a | 123 s |
| `cargo build --profile release-checked -j4`, crate only | 103 s | 70-97 s (unchanged, it now inherits `release-nolto`) |
| peak RSS of the build | 3.3 GB | 3.4 GB |
| `target/release/jsse` | 35.2 MB | 31.4 MB (-11%) |

CI (`ci.yml`, GitHub runner, rust-cache): the `Build` step was 54 s; expect about +2.5 min for the fat-LTO
build. Test steps move to `release-nolto` so they do not pay the LTO relink (`cargo test --release` links the
lib, the bin and each of the 12 integration tests against the release profile; measured +164 s without this).
Net CI wall time is about +2.5 min on a 12.5 min job; the nightly coverage job pins LTO off by environment
variable and the mutation-testing oracle uses `release-nolto`.

Startup: 15.6 ms -> 14.8 ms (empty script, median of 21), no regression from the larger inlining.

## Micro suite (wall seconds, median of N interleaved runs)

| benchmark | base | fat LTO + cgu=1 | ratio | N |
|---|---:|---:|---:|---:|
| array | 0.2549 | 0.2379 | 0.933 | 21 |
| closure_create | 0.8718 | 0.7895 | 0.906 | 5 |
| closures | 1.8291 | 1.6641 | 0.910 | 5 |
| fib | 1.6902 | 1.6556 | 0.980 | 5 |
| json | 0.1436 | 0.1365 | 0.951 | 21 |
| loop | 0.6607 | 0.6354 | 0.962 | 5 |
| object | 0.3860 | 0.3818 | 0.989 | 21 |
| opmix | 11.8977 | 11.5210 | 0.968 | 5 |
| promise_new | 1.8023 | 1.6418 | 0.911 | 5 |
| regex | 0.0631 | 0.0575 | 0.911 | 21 |
| string | 0.0837 | 0.0788 | 0.941 | 21 |

Geomean ratio 0.942, median 0.941, worst 0.989. Startup (empty script, N=41 / N=21): 25.6 ms -> 24.2 ms; 15.6 -> 14.8 ms.

## JetStream 3, requested subset (7 benchmarks, median of 5 interleaved rounds, score higher is better)

| benchmark | base | fat LTO + cgu=1 | ratio |
|---|---:|---:|---:|
| Babylon | 4.181 | 4.433 | 1.060 |
| raytrace | 0.995 | 1.057 | 1.062 |
| richards | 1.276 | 1.340 | 1.050 |
| splay | 3.706 | 3.937 | 1.062 |
| hash-map | 0.803 | 0.844 | 1.051 |
| bigint-noble-secp256k1 | 2.433 | 2.655 | 1.091 |
| lazy-collections | 1.653 | 1.819 | 1.100 |

Geomean ratio 1.068; 7 of 7 faster, worst 1.050.

## JetStream 3, all 48 benchmarks (median of 3 interleaved rounds, score higher is better)

| benchmark | base | fat LTO + cgu=1 | ratio |
|---|---:|---:|---:|
| Air | 6.527 | 6.859 | 1.051 |
| Basic | 3.458 | 3.748 | 1.084 |
| ML | 0.276 | 0.292 | 1.058 |
| Babylon | 4.188 | 4.429 | 1.058 |
| cdjs | 0.615 | 0.642 | 1.045 |
| first-inspector-code-load | 18.939 | 19.380 | 1.023 |
| multi-inspector-code-load | 19.011 | 20.000 | 1.052 |
| Box2D | 2.229 | 2.387 | 1.071 |
| octane-code-load | 92.593 | 89.286 | 0.964 |
| crypto | 3.748 | 3.959 | 1.056 |
| delta-blue | 1.933 | 2.068 | 1.069 |
| earley-boyer | 2.668 | 2.756 | 1.033 |
| gbemu | 0.671 | 0.721 | 1.075 |
| mandreel | 0.189 | 0.184 | 0.974 |
| navier-stokes | 6.274 | 6.840 | 1.090 |
| pdfjs | 2.245 | 2.382 | 1.061 |
| raytrace | 0.986 | 1.041 | 1.056 |
| regexp-octane | 3.949 | 4.417 | 1.118 |
| richards | 1.273 | 1.328 | 1.044 |
| splay | 3.720 | 3.925 | 1.055 |
| typescript-octane | 0.158 | 0.165 | 1.042 |
| FlightPlanner | 9.141 | 9.488 | 1.038 |
| OfflineAssembler | 0.428 | 0.486 | 1.136 |
| UniPoker | 3.687 | 3.937 | 1.068 |
| validatorjs | 8.681 | 9.524 | 1.097 |
| hash-map | 0.806 | 0.843 | 1.046 |
| doxbee-promise | 2.639 | 2.662 | 1.009 |
| doxbee-async | 4.651 | 5.030 | 1.081 |
| ai-astar | 3.432 | 3.628 | 1.057 |
| gaussian-blur | 1.059 | 1.058 | 0.999 |
| stanford-crypto-aes | 3.034 | 3.247 | 1.070 |
| stanford-crypto-pbkdf2 | 4.739 | 5.118 | 1.080 |
| stanford-crypto-sha256 | 4.409 | 4.744 | 1.076 |
| json-stringify-inspector | 13.263 | 14.006 | 1.056 |
| json-parse-inspector | 4.682 | 4.587 | 0.980 |
| bigint-noble-bls12-381 | 0.199 | 0.213 | 1.071 |
| bigint-noble-secp256k1 | 2.426 | 2.674 | 1.102 |
| bigint-noble-ed25519 | 2.102 | 2.306 | 1.097 |
| bigint-paillier | 1.832 | 1.994 | 1.089 |
| bigint-bigdenary | 2.530 | 2.728 | 1.078 |
| proxy-mobx | 16.393 | 17.794 | 1.085 |
| proxy-vue | 9.488 | 9.980 | 1.052 |
| raytrace-public-class-fields | 0.875 | 0.955 | 1.091 |
| raytrace-private-class-fields | 0.905 | 0.961 | 1.062 |
| async-fs | 2.294 | 2.589 | 1.129 |
| sync-fs | 1.493 | 1.654 | 1.108 |
| lazy-collections | 1.635 | 1.822 | 1.114 |
| js-tokens | 1.483 | 1.706 | 1.150 |

Geomean ratio 1.064; 44 of 48 faster, worst 0.964.

## Versus boa 0.22.0 (micro suite, median of 5, LTO jsse)

boa/jsse wall ratio: array 1.29, closure_create 0.75, closures 1.26, fib 0.72, json 2.23, loop 2.91, object 1.43,
opmix 1.10, promise_new 1.10 (checksum line differs textually between engines), regex 1.96, string 4.63; geomean
1.50 (jsse faster overall; fib and closure_create remain slower than boa).

## Gates on the LTO binary

- `scripts/run-test262.py` (full, 99,911 scenarios): 100% pass, 0 regressions.
- `scripts/run-custom-tests.py`: 18/18 (including `tests/recursion-limit-*.js`, so frame sizes are fine).
- `JSSE_GC_STRESS=7` over `test262-extra/`: 1343/1343.
