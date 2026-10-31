# Library Tests: TypeScript `conformance` runner, es5 / es7 / es2016-es2025 / esnext (#1278)

Part of [library-tests-typescript.md](library-tests-typescript.md); the oracle, baseline IO, control and ratchet are those of [library-tests-typescript-compiler.md](library-tests-typescript-compiler.md), the conformance wiring that of [library-tests-typescript-conformance-expressions.md](library-tests-typescript-conformance-expressions.md).

**What it is**: `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-es-oracle.py [--chunks es-a,es-b,es-c,es-d] [--check-partition] [-- oracle options]` runs the `conformance` runner for the 13 directories `es5`, `es7`, `es2016` to `es2025` and `esnext` of `tests/cases/conformance` (226 suites of 6 tests = the 1356 passing tests `ts-suite/oracle.json` pins for them: es5 12, es7 402, es2016 6, es2017 114, es2018 42, es2019 132, es2020 210, es2021 234, es2022 120, es2023 18, es2024 18, es2025 24, esnext 24) through mocha and `built/local/run.js` on Node and `jsse --node`, and compares them test by test plus every file under `tests/baselines/local`. `es6` and `esDecorators` are other tickets. The wrapper loads `scripts/run-ts-conformance-expressions-oracle.py` (suite titles, `dir:` slices, the `oracle.json` pins, the oracle loader) and only changes the group: `scripts/ts_conformance_slices_1278.py` adds the rows `issue1278`, one per directory, and four chunks passed with `--slices-module`.

**Chunks** (run one after the other so a crashed chunk does not lose the others): `es-a` (es5, es7: 69 suites, 414 tests), `es-b` (es2016-es2019: 49, 294), `es-c` (es2020, es2021: 74, 444), `es-d` (es2022-es2025, esnext: 34, 204). `--check-partition` (Node only; `tests/node_ts_conformance_es.rs`, `scripts/test_ts_conformance_slices_1278.py`) fails unless the chunks cover `issue1278` exactly once. Options after `--` go to the oracle (`--jobs` at most 4, `--batch-size`, `--jsse`, `--record`, `--verbose`, `--no-control`, `--no-jsse`).

**Known failures**: `ts-suite/conformance-es-known-failures.txt` (`full test title<TAB>issue`, filed under the tracker #1418; `--record` rewrites it). It has no entries: a test passing on Node and failing on jsse fails the run until it is fixed or listed, and a listed test that passes fails it too.

**Measured** (jsse built from 793fe70d with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4 --verbose`, default batch size 40, a shared 24-core host):

| chunk | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|
| es-a | 414 | 414 pass | 414 pass | 6 s | 311 s | 160 s |
| es-b | 294 | 294 pass | 294 pass | 7 s | 249 s | 135 s |
| es-c | 444 | 444 pass | 444 pass | 6 s | 380 s | 195 s |
| es-d | 204 | 204 pass | 204 pass | 3 s | 117 s | 119 s |
| total | 1356 | 1356 pass, 0 fail | 1356 pass, 0 fail | 22 s | 1057 s | 609 s |

Per-test outcomes agree and neither engine writes a file under `tests/baselines/local` in the real slice, so no engine or host change was needed. The 7-suite corrupted-reference control of each chunk (reference baselines appended to, replaced, removed, or added for a baseline the test does not produce) fails the same tests with the same messages and the same local baselines on both engines.
