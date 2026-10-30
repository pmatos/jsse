# Library Tests: TypeScript `conformance` runner, module directories

Split from [library-tests-typescript.md](library-tests-typescript.md) to keep each file under the 20,000-byte budget.

**What it is** (#1283): `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-1283-oracle.py [--chunks mr,node,ext,im] [--check-partition] [-- oracle options]` runs the `conformance` runner (`tests/cases/conformance`) for `moduleResolution` (534 passing tests), `node` (2058), `externalModules` (1818), `internalModules` (462), `importAssertion` (60), `importAttributes` (108), `importDefer` (162), `dynamicImport` (510) and `nonjsExtensions` (84): 966 suites, 5796 tests (the counts pinned per top-level directory in `ts-suite/oracle.json`), on Node and `jsse --node`. It reuses the machinery of [the expressions group](library-tests-typescript-conformance-expressions.md) and so of [the compiler oracle](library-tests-typescript-compiler.md): the script loads `scripts/run-ts-conformance-expressions-oracle.py` (which points `scripts/run-ts-compiler-oracle.py` at the conformance runner) and adds only the rows of `scripts/ts_conformance_slices_1283.py` (`--slices-module`) and its own ratchet file. Guards: `scripts/test_ts_conformance_slices_1283.py`, `tests/node_ts_conformance_1283.rs`.

**Chunks** (run one after the other, one oracle run each, so a crashed chunk loses nothing): `mr` (`moduleResolution`, `importAssertion`, `importAttributes`, `importDefer`, `nonjsExtensions`: 158 suites, 948 tests), `node` (343, 2058), `ext` (`externalModules`: 303, 1818) and `im` (`internalModules`, `dynamicImport`: 162, 972). `--check-partition` (Node only) fails unless the chunks select disjoint suites that add up to the row `issue1283` (966), and that six tests per suite equal the `oracle.json` pins of the group and of every chunk (5796; 948, 2058, 1818, 972).

**Compared**: the outcome of every test, the error message of a test failing on both, every file under `tests/baselines/local` byte for byte, and the seven-suite corrupted-reference control (it must fail identically on both engines). A test passing on Node and failing on jsse must be listed in `ts-suite/conformance-1283-known-failures.txt` (`full test title<TAB>issue`, filed under the tracker #1418; `--record` rewrites it); a listed test that passes fails the run. The file has no entries.

**Measured** (jsse built from 507cdc78 with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4`, 40 suites per batch, a shared 24-core host at load average 7 to 9; command `scripts/run-ts-conformance-1283-oracle.py -- --jobs 4 --verbose`):

| chunk | suites | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|---|
| mr | 158 | 948 | 948 pass | 948 pass | 10 s | 419 s | 182 s |
| node | 343 | 2058 | 2058 pass | 2058 pass | 31 s | 1410 s | 454 s |
| ext | 303 | 1818 | 1818 pass | 1818 pass | 26 s | 1074 s | 357 s |
| im | 162 | 972 | 972 pass | 972 pass | 13 s | 558 s | 221 s |
| total | 966 | 5796 | 5796 pass, 0 fail | 5796 pass, 0 fail | 80 s | 3461 s | 1214 s |

Per-test outcomes agree and no `tests/baselines/local` file is written by either engine in the real slices, so no engine or host change was needed. The control of every chunk fails the same tests (4, 6, 7 and 7) with the same messages and the same local baselines on both engines.
