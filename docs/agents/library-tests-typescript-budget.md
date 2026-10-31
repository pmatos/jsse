# Library Tests: TypeScript suite wall-time and memory budget

Split from [library-tests-typescript.md](library-tests-typescript.md) to keep each file under the 20,000-byte budget. Issue #1297 (epic #1214).

## The budget

| resource | rule | enforced by |
| --- | --- | --- |
| peak RSS | jsse at most **6x Node** for the same selection of the suite (`max_rss_ratio`, same cap as the `tsc` self-compile budget) | `scripts/run-ts-budget-oracle.py`, per case |
| wall time | **no cap** (ruling on #1297). Recorded per case and printed as a ratio to Node; the shipped budget file may not set a time limit (`--check-budget` rejects one) | recorded only |
| correctness | every test that passes on Node passes on jsse (or is in `ts-suite/budget-1297-known-failures.txt`, empty; a listed test that passes fails the run) | same script |

Planning figures for the whole suite (106,367 tests of `ts-suite/oracle.json`), from the per-range runs recorded in the other `library-tests-typescript-*.md` files (jsse built with `CARGO_PROFILE_RELEASE_LTO=off`, shared 24-core host, process time = sum over the mocha processes):

| runner | tests | Node process time | jsse process time | source |
| --- | ---: | ---: | ---: | --- |
| compiler 0-9,A-F,M-Z | 35,808 | ~600 s | 40,800 s | compiler doc (15253 + 12213 + 7241 + 6094) |
| compiler G-L | 7,848 | ~130 s | ~8,900 s (estimate, 1.14 s/test of the other ranges) | not recorded |
| conformance (all 11 directory groups) | 46,182 | ~700 s | 32,800 s | the conformance docs |
| fourslash + fourslash-server | 6,810 | ~200 s | 5,050 s | fourslash doc |
| project + transpile | 2,565 | ~20 s | 620 s | project, transpile docs |
| unit tests | 7,154 | ~250 s | ~6,600 s (services 848 s, tsbuild 764, evaluation 266, tscWatch 294 measured here; the rest from the wall times in the unit-test docs) | unit-test docs |
| **whole suite** | **106,367** | **~30 minutes** | **~95,000 s (about 26 hours)** | about 6.6 hours of wall time at the 4-process limit |

jsse is 10x to 60x Node on wall time per case (table below) and the ratio is steady across runners: the work is the interpreter running TypeScript's own checker, not any one suite. Two fixed costs frame every process: **about 30 s and 1.7 GiB** to load the 13 MB `built/local/run.js` and register the tests (Node: 1.5 s, 531 MiB), then another **60 to 85 s** for the first `Program` (lib.d.ts parse and bind). Batching many suites per process (the oracles' `--batch-size`) is therefore the main throughput lever; see the profile below for the engine ones.

## The check

`TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-budget-oracle.py [--smoke] [--jobs N (at most 4)] [--report FILE] [--budget FILE] [--known FILE] [--timeout S] [--test-timeout MS] [case ...]` runs each case as one mocha process (`node_modules/mocha/bin/_mocha`, `built/local/run.js`, runners from `test.config`, `--grep`) on Node and then on `jsse --node` in a private working directory of symlinks (the checkout is never written to; `tests/baselines/local` is the run's own), measures wall time and the `ru_maxrss` of the process tree through `wait4` (Linux folds the driver's own high-water mark into a vfork child's at exec; the driver is tens of MiB against 213 MiB for the smallest measured process, so no ratio moves) (killing the whole process group on exit, timeout or signal, `scripts/oracle_procs.py`), and prints a table with both ratios and tests per second. Exit 1 on a test that passes on Node and not on jsse, a Node failure, an exit status that does not match the failures mocha reported (a crash after the report), a case that selects nothing, or a case over its budget; `--smoke` and named cases are mutually exclusive. `--check-budget` validates `ts-suite/budget-1297.json` against the case table and needs no checkout; without `TS_SRC_DIR` the run skips (`--require` makes that exit 3). The TypeScript tree comes from `uv run python scripts/run-ts-suite-oracle.py --dir DIR fetch install build` (the pinned `v6.0.3`, `ts-suite/README.md`), then `TS_SRC_DIR=DIR/ts-src`.

Files: `scripts/run-ts-budget-oracle.py` (driver), `scripts/ts_budget_cases.py` (the cases), `scripts/ts_budget_check.py` (measurement and budget arithmetic), `ts-suite/budget-1297.json` (per case `max_rss_ratio: 6`; `jsse_max_rss_mb`, `jsse_max_seconds` and `max_time_ratio` exist for a local override via `--budget` and are null), `ts-suite/budget-1297-known-failures.txt`. Guards: `scripts/test_ts_budget_1297.py` (outcome comparison, ratchet, budget arithmetic, the shipped file's ruled limits, measurement and timeout), `tests/node_ts_budget_1297.rs` (budget file; `bundle-load` and `transpile` end to end when `TS_SRC_DIR` is set). The failing path was checked by hand with a 1.5x cap on `transpile` (3.05x measured: `FAIL ... over the budget of 1.5x`, exit 1).

**CI.** `.github/workflows/ts-suite-budget-smoke.yml` (non-required, on pull requests and pushes touching the engine, `node-lib/` or the budget files; shares nothing with the required `build` check) builds jsse with LTO off, fetches and builds the pinned TypeScript tree and runs `--smoke`: `bundle-load`, `transpile`, `compiler-first`. `.github/workflows/nightly-ts-suite-budget.yml` runs every case on the shipped release profile at 2 jobs and uploads the JSON report and log (wall times are in the step summary).

## Cases and measurements

`jsse` built from origin/main at 4eee3c40 with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, Node v26.8.2, 24-core host at load average 10 to 25 shared with other agents, smoke cases at 1 job and the others at 2 jobs (wall times include that contention; RSS does not):

| case | selection | tests | Node s | jsse s | wall x | Node MiB | jsse MiB | RSS x |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `bundle-load` (smoke) | `unittests:: debugDeprecation` | 6 | 1.5 | 31.9 | 21.9 | 531 | 1668 | 3.14 |
| `transpile` (smoke) | the transpile runner | 37 | 0.5 | 5.1 | 10.2 | 213 | 665 | 3.13 |
| `compiler-first` (smoke) | compiler `a[a-b]*` | 60 | 2.0 | 115.9 | 59.3 | 552 | 2542 | 4.60 |
| `compiler-366` | compiler `a[a-f]*`, `b[a-c]*` | 366 | 3.9 | 162.0 | 41.9 | 676 | 2607 | 3.86 |
| `conformance-interfaces` | `conformance/interfaces/` | 396 | 3.9 | 172.6 | 44.7 | 677 | 2552 | 3.77 |
| `conformance-generators` | `conformance/generators/` | 102 | 3.8 | 161.1 | 42.2 | 583 | 2560 | 4.39 |
| `project-out` | project runner `*out*` | 200 | 2.2 | 43.3 | 19.6 | 327 | 662 | 2.02 |
| `fourslash-add` | fourslash `a[a-d]*` | 16 | 1.6 | 43.3 | 27.9 | 358 | 969 | 2.70 |
| `fourslash-server` | all server tests | 488 | 21.5 | 1138.5 | 52.9 | 943 | 726 | 0.77 |
| `tsserver-sourcemap` | tsserver `projectReferencesSourcemap` | 273 | 14.3 | 335.4 | 23.4 | 1097 | 1880 | 1.71 |
| `tscwatch` | all tscWatch | 375 | 11.7 | 294.5 | 25.1 | 1075 | 1939 | 1.80 |
| `tsbuild` | all tsbuild | 359 | 23.3 | 764.1 | 32.9 | 1457 | 1892 | 1.30 |
| `evaluation` | all evaluation | 350 | 7.4 | 265.6 | 35.8 | 1090 | 2940 | 2.70 |
| `services` | all services | 1531 | 18.3 | 848.1 | 46.2 | 1253 | 3296 | 2.63 |

All 14 cases pass on both engines with identical outcomes and no baseline written; no case is near the cap (worst 4.60x, `compiler-first`, where Node's RSS has not grown yet while jsse has loaded the bundle and the first `Program`; worst absolute 3.3 GiB, `services`). Peak RSS of a process does not grow with the number of tests once the lib files are cached (`compiler-first` 2.5 GiB for 60 tests, `compiler-366` 2.6 GiB for 366), so four jobs need about 13 GiB. The self-compile cases of `scripts/tsc-fixtures/selfcompile-budget.json` are the ones near the cap (5.7x to 6.6x, #1401).

## Profile and hot spots

`cargo build --profile release-nolto --features perf-counters` (`docs/agents/execution-counters.md`; counts are deterministic, the time share is the instrumented run's own):

| | `bundle-load` | `compiler-first` |
| --- | ---: | ---: |
| VM ops | 567 M | 1655 M |
| compiled / tree-walker function bodies | 11.4 M / 26 K | 47.7 M / 96 K |
| calls to native functions | 39.1 M | 86.5 M |
| minor / major collections | 1047 / 23 | 4118 / 48 |
| time in GC | 12.8 s of 28 s (45%) | 64.6 s of 124 s (52%) |
| average minor collection | 9.7 ms | 12.3 ms |
| `SetCallPos` ops (`--node` only) | 39.8 M (7.0%) | 95.1 M (5.75%) |
| `CaseStrictEq` ops | n/a | 42.8 M (2.6%) |

Reading: loading the runner bundle alone is 30 s (parse is 1.1 s and 340 MiB, the rest is executing the module bodies that register the ~7,000 unit tests), and a first `Program` adds 85 s. Hot spots, filed as sub-issues of #1418:

1. **#1490 GC**: about half the wall time of every process is garbage collection, nearly all of it minor collections costing 10 to 12 ms for a 4 MiB nursery (suspects: per-minor root rebuild, arena-capacity-sized young bitmap, remembered set).
2. **#1491 `switch` lowering**: a linear `CaseStrictEq` chain; the scanner, parser and checker are switches over hundreds of `SyntaxKind` values.
3. **#1492 `SetCallPos`**: a separate op before every call under `--node`, 5.7 to 7% of executed ops.
4. **Memory** (existing): #1401 (self-compile RSS) and #1420 (closures retain their whole defining environment) are the levers on the 3 to 4.6x RSS ratios above; `bundle-load` alone is 1.7 GiB against Node's 0.5 GiB.

The tree-walker is not a hot spot here (3 M AST work units against 1.65 G VM ops; the `construct needs tree-walked frame` bail of `class ... extends` constructors using `super` covers `IdentifierObject`, `TokenObject` and `Buffer` but costs well under 1% of the run).

## Refreshing the numbers

Run `... run-ts-budget-oracle.py --jobs 2 --report r.json` on a quiet host with the shipped build, and replace the tables above; never compare timings across LTO configurations or against a counters build. The nightly artifact `ts-suite-budget-report` carries the same JSON.
