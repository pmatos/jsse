# TypeScript test suite pin and Node oracle (epic #1214, issue #1250)

`pin.json` fixes the TypeScript tree whose own test suite later tickets run on `jsse --node`:
`microsoft/TypeScript` tag `v6.0.3`, commit `050880ce59e30b356b686bd3144efe24f875ebc8`, and the sha256 of its
`package-lock.json` (installed with `npm ci`). `oracle.json` is what Node produces on that tree.
Nothing from TypeScript is vendored: `scripts/run-ts-suite-oracle.py` keeps sources, `node_modules`, build output
and run logs in a cache directory outside the repository (`--dir`, else `$TS_SUITE_DIR`, else
`~/.cache/jsse-ts-suite`) and refuses one inside it.

    uv run python scripts/run-ts-suite-oracle.py                 # fetch install build run check
    uv run python scripts/run-ts-suite-oracle.py --offline       # run the suites in an empty network namespace
    uv run python scripts/run-ts-suite-oracle.py record          # rewrite oracle.json from runs/latest
    uv run python scripts/run-ts-suite-oracle.py check --run-dir DIR

About 6 minutes on a 24-core host (clone and `npm ci` under 10 s, `hereby local` 20 s, `hereby tests` 7 s, the suites
2 to 4 minutes with 8 workers, the eslint rules under 1 s). The script verifies the cloned commit and the lockfile
digest before building, and `check` exits 1 when any count or digest differs from `oracle.json`.

## What is recorded

`oracle.json`: the engine used (Node v26.8.2, npm 11.19.1), and per runner the passing and failing test cases, the
test files, and counts per sub-bucket: `unittests` per suite (`tsserver`, `services`, `config`, ...), `conformance`
per directory, `compiler`, `fourslash` and `fourslash-server` per upper-cased initial of the test file (the ranges
the epic's child tickets use). Every runner also carries the sha256 of its sorted list of passing test ids, so a
later jsse run localises a divergence to a runner without the list. Totals on Node: 106,367 passing, 0 failing, plus
79 eslint-rule tests (327 unit-test suites and 19,676 runner test files discovered).

**The full pass list is not committed.** 106k ids are 21.8 MB, 457 KB with xz at best: about 25 parts of 19,000
bytes for one baseline. The run writes it to `runs/<stamp>/node-pass-list.txt` (sorted, one id per line, `\`, newline
and CR escaped, duplicates kept: 110 ids repeat) and `oracle.json` pins its digest as `totals.digest`. The Node run
is deterministic (three runs with 8 workers, in separate cache directories, online and in an empty network namespace, gave identical ids), so the baseline ticket
(#1296) can regenerate the list or commit a compressed form.

## Layout facts re-verified on v6.0.3

- Runners selected by `hereby runtests-parallel` (`src/testRunner/runner.ts`): the unit tests (327 mocha suites
  imported from `src/testRunner/tests.ts`), `conformance` and `compiler` (two `CompilerBaselineRunner`s), `project`,
  `fourslash`, `fourslash-server`, `transpile`. `fourslash-generated` exists but is off by default. The eslint rules
  are a separate task (`hereby run-eslint-rules-tests`), not a runner.
- The parallel runner is not mocha's parallel mode. `built/local/run.js` (a 13 MB esbuild bundle) is the host: it
  writes `test.config`, discovers tests, and `child_process.fork`s `process.argv[1]` once per worker with
  `--config="<tmpdir>/ts-testsN/task-configI.json"` over an IPC channel; workers get tasks by message and report
  results back; the host replays them into mocha's `Base` reporter. Mocha is 10.8.2, used as a library only.
- The only per-test listing in parallel mode is the xunit file the host writes when `CI=true`
  (`TEST-results.xml` in the cwd); otherwise it prints counts and writes `.failed-tests`. The script sets `CI=true`.
- `hereby runtests-parallel` also runs `hereby lint` unless `--no-lint` (so `npm test` includes eslint with
  typescript-eslint, #1295). The script passes `--no-lint`.
- The build type-checks with `node_modules/typescript` (5.9.3, via `tsc -b`), not with the tree under test, and
  bundles with esbuild 0.27.3 through its JS API (a native binary over stdio pipes).
- No test needs the network or git: the whole suite passes in an empty network namespace. The tests use virtual file
  systems and mock hosts; the TypeScript tree is only cloned to build it.
- Default per-test timeout is 40 s. `relationComplexityError.ts` exceeded it once at load average 97 with 12 workers,
  and passed at 8; pass `--timeout` for slow or loaded hosts.
- mocha 10.8.2's CLI (`node_modules/mocha/bin/_mocha`, used by `hereby runtests` and `run-eslint-rules-tests`)
  does not start on Node >= 22.12: it requires `yargs/yargs`, an extensionless CommonJS file in a `"type": "module"`
  package, which `require(esm)` now loads as ESM. The install step drops that one `type` key in the cache copy.
