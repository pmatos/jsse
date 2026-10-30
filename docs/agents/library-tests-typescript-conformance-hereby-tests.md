# Library Tests: `hereby tests`, the TypeScript test runner built on jsse (#1254)

Part of [library-tests-typescript.md](library-tests-typescript.md) (epic #1214, phase "Test build chain"); it follows the `hereby local` oracle of #1253 (`scripts/run-hereby-local-oracle.py`), whose helpers it loads.

**What it is**: `TS_SRC_DIR=<TypeScript v6.0.3 checkout, npm ci done> uv run python scripts/run-hereby-tests-oracle.py [--jsse BIN] [--variants typecheck,no-typecheck] [--runners a,b] [--jobs N (at most 4)] [--skip-list] [--work DIR] [--require]` copies the checkout once per engine and variant (never writing to `TS_SRC_DIR`) and runs `hereby tests` and `hereby tests --no-typecheck` on Node and on `jsse --node`. `hereby tests` is `src/testRunner` type-checked by `node_modules/typescript` (`tsc -b`, 1304 files under `built/`) plus the esbuild bundle `built/local/run.js` (13.2 MB); `--no-typecheck` is the bundle alone (112 files). Per variant it checks:

1. every `hereby` exits 0 and the normalised logs (sorted lines, durations, the copy's path and the interpreter masked) are equal;
2. every file under `built/` and the generated sources agree by path, size and sha256 (`run.js`, its source map and the `tsc -b` emit included);
3. mocha's `--dry-run` over `built/local/run.js` (one run per runner through `test.config`) lists the same tests, line for line and with the same exit status, for the Node-built bundle on Node, the jsse-built bundle on Node and the jsse-built bundle on jsse. The runners are `unittest` (the quirk of `src/testRunner/runner.ts`: `unittest` alone selects the default runners plus the unit tests, 106,367 tests, the oracle's total), `conformance`, `compiler`, `project`, `fourslash`, `fourslash-server`, `fourslash-generated` and `transpile`. A listing with no tests is a failure.

Without `TS_SRC_DIR` (or with another TypeScript version) it prints `SKIP` and exits 0 unless `--require`. All processes run through `scripts/oracle_procs.py`, so the whole process group is killed on timeout or interrupt. Guards: `scripts/test_run_hereby_tests_oracle.py` (always run), `tests/node_hereby_tests.rs` (drives the oracle when `TS_SRC_DIR` and `JSSE_HEREBY_TESTS_ORACLE` are set).

**Known failures.** None: no engine or host change was needed. Any difference makes the oracle exit 1; root-cause it, or file it under the tracker #1418.

**Measured** (jsse built with `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, a shared 24-core host):

| variant | Node | jsse | files under `built/` | `run.js` |
|---|---|---|---|---|
| `hereby tests` | 10.2 s | 536.7 s | 1304 = 1304 | identical |
| `hereby tests --no-typecheck` | 0.5 s | 1.6 s | 112 = 112 | identical |

| runner | tests listed (identical on Node-built/Node, jsse-built/Node, jsse-built/jsse, both variants) |
|---|---|
| unittest (default runners plus unit tests) | 106367 |
| conformance | 46183 |
| compiler | 89839 |
| project | 2529 |
| fourslash | 6323 |
| fourslash-server | 489 |
| fourslash-generated | 1 |
| transpile | 38 |

The type-check dominates: `tsc -b src/testRunner` checks the whole compiler, services and harness, and runs about 50 times slower on jsse than on Node. The two bundles are byte-identical, so the runner that [library-tests-typescript.md](library-tests-typescript.md) oracles execute is the same whichever engine built it.
