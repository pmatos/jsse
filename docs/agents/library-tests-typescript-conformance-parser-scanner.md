# Library Tests: TypeScript `conformance` runner, parser / scanner / directives / pedantic / references (#1280)

Part of [library-tests-typescript.md](library-tests-typescript.md); the oracle, baseline IO, control and ratchet are those of [library-tests-typescript-compiler.md](library-tests-typescript-compiler.md), and the wrapper is that of [library-tests-typescript-conformance-classes-decorators.md](library-tests-typescript-conformance-classes-decorators.md).

**What it is**: `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-1280-oracle.py [--chunks cf1,cf2,...] [--check-partition] [slice ...] [-- oracle options]` runs the `conformance` runner (`tests/cases/conformance`) for the directories `parser` (5430 tests), `scanner` (264), `directives` (36), `pedantic` (12) and `references` (90), 972 suites and 5832 tests in all (the passing counts `ts-suite/oracle.json` pins per directory), through mocha and `built/local/run.js` on Node and `jsse --node`, and compares them test by test plus every file under `tests/baselines/local`. The script loads `scripts/run-ts-conformance-oracle.py` (#1276) and swaps in the slice module `scripts/ts_conformance_slices_1280.py` and the ratchet file `ts-suite/conformance-1280-known-failures.txt`; nothing of the shared wrappers is edited. Skips without `TS_SRC_DIR` (the wrapper always passes `--require`, so it fails instead). Guards: `scripts/test_ts_conformance_slices_1280.py`, `tests/node_ts_conformance_1280.rs`.

**Chunks** (rows of the slice module, run one oracle run after the other so a crashed chunk loses nothing; each prints its wall time): `cf1` (directives, pedantic, references, scanner, parser/ecmascript2018, 2021, 3 and 6; 176 suites, 1056 tests), `cf2` (the files directly in parser/ecmascript5 plus its subdirectories Accessors..EnumDeclarations; 197, 1182), `cf3` (ErrorRecovery..IndexMemberDeclarations; 188, 1128), `cf4` (IndexSignatures..RegressionTests; 186, 1116) and `cf5` (RegularExpressions..VariableDeclarations; 225, 1350). The extra slice term `only:parser/ecmascript5` selects the suites lying directly in a directory (`dir:` takes its subdirectories too), and the rows `parser`, `scanner`, `directives`, `pedantic`, `references` and `issue1280` select whole directories. `--check-partition` (Node only) fails unless the chunks select disjoint suites that add up to `issue1280`, so a subdirectory added upstream and named by no chunk is reported.

**Known failures.** A test passing on Node and failing on jsse must be listed in `ts-suite/conformance-1280-known-failures.txt` (`full test title<TAB>issue`, filed under the tracker #1418; `--record` rewrites it); a listed test that passes fails the run. The file has no entries.

**Measured** (jsse built from 793fe70d with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4`, 40 suites per batch, a shared 24-core host; command `scripts/run-ts-conformance-1280-oracle.py -- --jobs 4 --verbose`). Outcomes, error messages and local baselines compared between the engines; the control (7 suites of each chunk re-run against corrupted reference baselines: appended to, replaced, removed, or a reference added for a baseline the test does not produce) fails the same tests with the same messages and local baselines on both engines in every chunk:

| chunk | suites | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|---|
| cf1 | 176 | 1056 | 1056 pass | 1056 pass | 15 s | 671 s | 252 s |
| cf2 | 197 | 1182 | 1182 pass | 1182 pass | 15 s | 831 s | 317 s |
| cf3 | 188 | 1128 | 1128 pass | 1128 pass | 14 s | 678 s | 245 s |
| cf4 | 186 | 1116 | 1116 pass | 1116 pass | 13 s | 597 s | 222 s |
| cf5 | 225 | 1350 | 1350 pass | 1350 pass | 17 s | 697 s | 242 s |
| total | 972 | 5832 | 5832 pass, 0 fail | 5832 pass, 0 fail | 74 s | 3474 s | 1278 s |

Per-test outcomes agree, no test fails on either engine, and the real slice writes no file under `tests/baselines/local` on either engine (only the control does, identically), so no engine or host change was needed. A regression shows up as a test passing on Node and failing on jsse, which the oracle reports until it is fixed or filed under #1418 and listed in the known-failures file.
