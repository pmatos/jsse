# Library Tests: TypeScript `conformance` runner, declarationEmit / emitter / ambient / additionalChecks (#1281)

Part of [library-tests-typescript.md](library-tests-typescript.md); the oracle, baseline IO, control and ratchet are those of [library-tests-typescript-compiler.md](library-tests-typescript-compiler.md), and the wiring for a `conformance` directory is that of [library-tests-typescript-conformance-classes-decorators.md](library-tests-typescript-conformance-classes-decorators.md).

**What it is**: `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-1281-oracle.py [--dirs d1,d2] [--check-partition] [slice ...] [-- oracle options]` runs the `conformance` runner for `declarationEmit` (23 suites, 138 tests), `emitter` (17, 102), `ambient` (22, 132) and `additionalChecks` (1, 6): 63 suites and the 378 tests `ts-suite/oracle.json` pins for the four directories, through mocha and `built/local/run.js` on Node and `jsse --node`, comparing them test by test plus every file under `tests/baselines/local`. The script loads `scripts/run-ts-conformance-oracle.py` (#1276) and swaps in the slice module `scripts/ts_conformance_slices_1281.py` (rows `declarationEmit`, `emitter`, `ambient`, `additionalChecks`, `issue1281`; every other term is that of the compiler or #1276 slices) and the known-failures file `ts-suite/conformance-1281-known-failures.txt` (empty). Without slices it runs the four directories one after the other, one oracle run each, and prints each wall time. `--check-partition` (Node only; `tests/node_ts_conformance_1281.rs`, `scripts/test_ts_conformance_slices_1281.py`) fails unless the four directories select disjoint suites that add up to `issue1281`.

**Measured** (jsse built from 0b7507cc with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4`, one batch of at most 40 suites per directory, a shared 24-core host; command `scripts/run-ts-conformance-1281-oracle.py -- --jobs 4 --verbose`):

| directory | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|
| declarationEmit | 138 | 138 pass | 138 pass | 3 s | 133 s | 136 s |
| emitter | 102 | 102 pass | 102 pass | 4 s | 130 s | 132 s |
| ambient | 132 | 132 pass | 132 pass | 3 s | 121 s | 122 s |
| additionalChecks | 6 | 6 pass | 6 pass | 2 s | 113 s | 114 s |
| total | 378 | 378 pass, 0 fail | 378 pass, 0 fail | 12 s | 497 s | 504 s |

Per-test outcomes agree, no `tests/baselines/local` file is written by either engine in the real slices, and the corrupted-reference control (7 suites per directory, 1 for additionalChecks, re-run against appended-to, rewritten, removed and added references) fails the same tests with the same messages and baselines on both engines. Nearly all of a jsse process is the load of the 13 MB runner bundle and the first `Program`. No engine or host change was needed; a regression shows up as a test that passes on Node and fails on jsse, which the oracle reports until it is fixed or filed under #1418 and listed in the known-failures file.
