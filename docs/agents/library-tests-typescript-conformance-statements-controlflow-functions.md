# Library Tests: TypeScript `conformance` runner, statements / controlFlow / functions

Split from [library-tests-typescript.md](library-tests-typescript.md) to keep each file under the 20,000-byte budget.

**What it is** (#1279): `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-1279-oracle.py [--chunks st1,st2,st3,cf,fn] [--check-partition] [-- oracle options]` runs the `conformance` runner (`tests/cases/conformance`) for the directories `statements` (525 suites, 3150 tests), `controlFlow` (58, 348) and `functions` (25, 150), 608 suites and 3648 tests in all (the counts pinned per top-level directory in `ts-suite/oracle.json`), on Node and `jsse --node`. It reuses the machinery of [the expressions group](library-tests-typescript-conformance-expressions.md) and so of [the compiler oracle](library-tests-typescript-compiler.md): the script loads `scripts/run-ts-conformance-expressions-oracle.py` (which points `scripts/run-ts-compiler-oracle.py` at the conformance runner) and adds only the rows of `scripts/ts_conformance_slices_1279.py` (`--slices-module`) and its own ratchet file. Guards: `scripts/test_ts_conformance_slices_1279.py`, `tests/node_ts_conformance_1279.rs`.

**Chunks** (run one after the other, one oracle run each, so a crashed chunk loses nothing): `st1` (`statements/VariableStatements`: 348 suites, 2088 tests), `st2` (`statements/for-await-ofStatements`, `for-inStatements`, `for-ofStatements`, `forStatements`: 131, 786), `st3` (the other nine statements directories: 46, 276), `cf` (`controlFlow`: 58, 348) and `fn` (`functions`: 25, 150). `--check-partition` (Node only) fails unless the chunks select disjoint suites that add up to the row `issue1279` (608), and that six tests per suite equal the `oracle.json` pins of `issue1279` (3648) and `statements` (3150), which the chunk runs themselves cannot check (the pin lookup knows whole top-level directories only).

**Compared**: the outcome of every test, the error message of a test failing on both, every file under `tests/baselines/local` byte for byte, and the seven-suite corrupted-reference control (it must fail identically on both engines). A test passing on Node and failing on jsse must be listed in `ts-suite/conformance-1279-known-failures.txt` (`full test title<TAB>issue`, filed under the tracker #1418; `--record` rewrites it); a listed test that passes fails the run. The file has no entries.

**Measured** (jsse built from 793fe70d with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4`, 40 suites per batch, a shared 24-core host at load average 13 to 24; command `scripts/run-ts-conformance-1279-oracle.py -- --jobs 4 --verbose`):

| chunk | suites | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|---|
| st1 | 348 | 2088 | 2088 pass | 2088 pass | 26 s | 885 s | 313 s |
| st2 | 131 | 786 | 786 pass | 786 pass | 13 s | 574 s | 239 s |
| st3 | 46 | 276 | 276 pass | 276 pass | 5 s | 187 s | 106 s |
| cf | 58 | 348 | 348 pass | 348 pass | 7 s | 279 s | 153 s |
| fn | 25 | 150 | 150 pass | 150 pass | 3 s | 109 s | 111 s |
| total | 608 | 3648 | 3648 pass, 0 fail | 3648 pass, 0 fail | 54 s | 2034 s | 922 s |

Per-test outcomes agree and no `tests/baselines/local` file is written by either engine in the real slices, so no engine or host change was needed. The control of every chunk fails the same tests (6 or 7 per chunk) with the same messages and the same local baselines on both engines.
