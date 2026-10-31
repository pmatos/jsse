# Library Tests: TypeScript `conformance` runner, `es6` (#1277)

Part of [library-tests-typescript.md](library-tests-typescript.md); the oracle, baseline IO, control and ratchet are those of [library-tests-typescript-compiler.md](library-tests-typescript-compiler.md), and the conformance wiring is that of [library-tests-typescript-conformance-classes-decorators.md](library-tests-typescript-conformance-classes-decorators.md).

**What it is**: `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-es6-oracle.py [--chunks es1,es2,es3,es4,es5] [--check-partition] [-- oracle options]` runs the `conformance` runner for `tests/cases/conformance/es6` (1315 suites of 6 tests = the 7890 tests of `ts-suite/oracle.json`) through mocha and `built/local/run.js` on Node and `jsse --node` and compares them test by test, plus every file under `tests/baselines/local`. The script loads `scripts/run-ts-conformance-oracle.py` (#1276) for `configure()` and `oracle()`, so the runner, suite titles, `dir:` slices and pin lookup are the existing ones; it adds only the chunk rows of `scripts/ts_conformance_slices_1277.py` (passed as `--slices-module`) and its own known-failure file. A `dir:es6/<subdirectory>` slice also works with the #1276 wrapper.

**Chunks** (each one oracle run, so a crashed one loses nothing): `es1` (Symbols, arrowFunction, binaryAndOctalIntegerLiteral, classDeclaration, classExpressions, decorators, defaultParameters; 267 suites), `es2` (computedProperties, for-ofStatements, functionDeclarations, functionExpressions, functionPropertyAssignments, memberFunctionDeclarations; 300), `es3` (destructuring, modules, moduleExportsAmd/Commonjs/System/Umd, newTarget, propertyAccess, restParameters; 284), `es4` (shorthandPropertyAssignment, spread, templates, variableDeclarations; 234), `es5` (unicodeExtendedEscapes, yieldExpressions; 230). `--check-partition` (Node only; `tests/node_ts_conformance_es6.rs`, `scripts/test_ts_conformance_slices_1277.py`) fails unless the chunks cover `es6` exactly once and the 7890 tests of `es6` match the count pinned in `ts-suite/oracle.json` (chunk slices name subdirectories, which the oracle's own pin check skips); it takes no `--chunks` or oracle options.

**Known failures.** A test passing on Node and failing on jsse must be listed in `ts-suite/conformance-es6-known-failures.txt` (`full test title<TAB>issue`, filed under the tracker #1418; `--record` rewrites it); a listed test that passes fails the run. The file has no entries.

**Measured** (jsse built with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4`, default batch size 40, a shared 24-core host; command `scripts/run-ts-conformance-es6-oracle.py -- --jobs 4 --verbose`). The corrupted-reference control (7 suites of each chunk re-run against appended, replaced, removed and added reference baselines) fails identically on both engines in every chunk:

| chunk | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|
| es1 | 1602 | 1602 pass | 1602 pass | 22 s | 825 s | 251 s |
| es2 | 1800 | 1800 pass | 1800 pass | 20 s | 1018 s | 350 s |
| es3 | 1704 | 1704 pass | 1704 pass | 20 s | 879 s | 308 s |
| es4 | 1404 | 1404 pass | 1404 pass | 14 s | 674 s | 215 s |
| es5 | 1380 | 1380 pass | 1380 pass | 15 s | 519 s | 183 s |
| total | 7890 | 7890 pass, 0 fail | 7890 pass, 0 fail | 91 s | 3915 s | 1307 s |

Per-test outcomes, error messages and the baselines written under `tests/baselines/local` (only the control's own, identical on both engines) agree, so no engine or host change was needed. A regression shows up as a test passing on Node and failing on jsse, which the oracle reports until it is fixed or filed under #1418 and listed in the known-failures file.
