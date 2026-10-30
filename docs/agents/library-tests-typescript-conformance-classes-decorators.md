# Library Tests: TypeScript `conformance` runner, classes / decorators / esDecorators / override

Split from [library-tests-typescript.md](library-tests-typescript.md) to keep each file under the 20,000-byte budget.

**What it is** (#1276): `TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-conformance-oracle.py [--dirs d1,d2] [--check-partition] [slice ...] [-- oracle options]` runs the `conformance` runner (`tests/cases/conformance/**`: 7697 suites of 6 tests, 46182 passing tests on Node in `ts-suite/oracle.json`) for the directories `override` (38 suites, 228 tests), `decorators` (140, 840), `esDecorators` (217, 1302) and `classes` (654, 3924), 1049 suites and 6294 tests in all, on Node and `jsse --node`. The runner is the compiler runner's `CompilerBaselineRunner` over another directory, so the script is [the compiler oracle](library-tests-typescript-compiler.md) (`scripts/run-ts-compiler-oracle.py`: private working directory of symlinks with its own empty `tests/baselines/local`, batches of many suites per mocha process, at most 4 processes at once, the per-test outcome, error-message and local-baseline comparison, the corrupted-reference control, the known-failure ratchet) pointed at the `conformance` runner. Its wrapper only swaps the runner name, the mocha `--grep`, the slice module and the pin lookup. Skips without `TS_SRC_DIR` (the wrapper always passes `--require`, so it fails instead). Guards: `scripts/test_ts_conformance_slices.py`, `tests/node_ts_conformance_1276.rs`.

**Slices** (`scripts/ts_conformance_slices.py`): the compiler oracle's terms (`first:N`, `prefix:`, `shard:I/N`, `files:`, `match:`) plus `dir:classes,decorators` (suites under `tests/cases/conformance/<dir>`, a sub-path such as `dir:classes/classDeclarations` works), and the rows `override`, `decorators`, `esDecorators`, `classes` and `issue1276` (the four). Positional slices before `--` run as one oracle run; with none, the wrapper runs the four directories in the order override, decorators, esDecorators, classes, one oracle run each (a crashed or interrupted one loses nothing) and prints each wall time. `ts-suite/oracle.json` pins the passing count per top-level directory, which the oracle checks for `dir:` slices naming whole top-level directories. `--check-partition` (Node only) fails unless the four directories select disjoint suites that add up to `issue1276`.

**Known failures.** A test passing on Node and failing on jsse must be listed in `ts-suite/conformance-1276-known-failures.txt` (`full test title<TAB>issue`, filed under the tracker #1418; `--record` rewrites it); a listed test that passes fails the run. The file has no entries.

**Measured** (jsse built from 22c4756b with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `--jobs 4`, 40 suites per batch, a shared 24-core host; command `scripts/run-ts-conformance-oracle.py -- --jobs 4 --verbose`). Outcomes, error messages and local baselines compared between the engines, control (7 suites of the slice re-run against corrupted reference baselines: appended to, replaced, removed, or a reference added for a baseline the test does not produce) failing identically on both engines in every directory:

| directory | tests | Node | jsse | Node process time | jsse process time | wall time |
|---|---|---|---|---|---|---|
| override | 228 | 228 pass | 228 pass | 9 s | 269 s | 275 s |
| decorators | 840 | 840 pass | 840 pass | 21 s | 1003 s | 543 s |
| esDecorators | 1302 | 1302 pass | 1302 pass | 27 s | 1982 s | 617 s |
| classes | 3924 | 3924 pass | 3924 pass | 54 s | 2821 s | 802 s |
| total | 6294 | 6294 pass, 0 fail | 6294 pass, 0 fail | 111 s | 6075 s | 2237 s |

Per-test outcomes agree, no test fails on either engine, and the baselines the runs write under `tests/baselines/local` are only the control's own (identical on both engines), so no engine or host change was needed. A regression shows up as a test passing on Node and failing on jsse, which the oracle reports until it is fixed or filed under #1418 and listed in the known-failures file.
