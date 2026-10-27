# Node host mode: pinned TypeScript suite and Node oracle

Status: accepted, epic #1214, issue #1250 (builds on #1243, `2026-10-27-0900-node-host-tsc-projects.md`).

## Decision

1. **One pin, nothing vendored.** `ts-suite/pin.json` names `microsoft/TypeScript` tag `v6.0.3`, its commit sha
   and the sha256 of `package-lock.json`. `scripts/run-ts-suite-oracle.py` clones the tag (`--depth 1`) into a cache
   directory (`--dir`, `$TS_SUITE_DIR`, default `~/.cache/jsse-ts-suite`; refused inside the repository), checks
   `HEAD` against the pinned commit and the lockfile against its digest (a moved tag or a changed lockfile is an
   error), runs `npm ci --ignore-scripts` (the esbuild and dprint postinstalls only optimise; esbuild's native
   binary ships as an optional dependency), then `hereby local` and `hereby tests`.
2. **Stages.** `fetch install build run record check`, default `fetch install build run check`. `run` executes
   `CI=true hereby runtests-parallel --light=false --no-lint --workers=N` and then
   `hereby run-eslint-rules-tests --reporter=xunit`, each into `runs/<stamp>/` with a log, an exit-code file and the
   xunit file. `--offline` wraps the run in `unshare -rn`, which proves nothing reaches the network.
   `--no-lint` because lint is its own ticket (#1295); `--light=false` because that is `npm test`.
   `build` and `run` re-verify the commit and lockfile first. `record` refuses a run with a non-zero hereby exit,
   failed or skipped tests; `check` reports them as differences.
3. **Why xunit.** In parallel mode the host prints only counts and writes failures to `.failed-tests`. With
   `CI=true` it also replays every result into mocha's xunit reporter, the only complete per-test list. A test id is
   the whitespace-normalised suite path plus the test title; 110 ids repeat, so the list is a multiset.
4. **Manifest, not the list.** `ts-suite/oracle.json` (11 KB) holds per-runner counts, test files, sub-bucket counts
   and the sha256 of each runner's sorted passing ids. The 106,367-id list (21.8 MB, 457 KB xz) stays in the cache run
   directory; it would be 25 binary parts of 19,000 bytes for one baseline. Node's run is deterministic across cache
   directories and online/offline (three runs, identical digests), so the manifest pins the list by digest and #1296
   decides how a baseline is stored.
5. **Mocha CLI patch.** mocha 10.8.2's CLI cannot start on Node >= 22.12 (extensionless CommonJS `yargs/yargs` in a
   `"type": "module"` package). The install step removes that key in the cache copy so `run-eslint-rules-tests`
   runs on Node. A jsse run of those tests (#1294) needs the same patch or a loader that mirrors Node.
6. **Hermetic tests.** `scripts/test_run_ts_suite_oracle.py` (picked up by the CI `unittest discover` step) covers
   runner classification, buckets, digests, the diff, the pin checks against a local git repository, the cache
   guard and the committed pin/manifest agreement. The full run needs the network and minutes, so CI does not run it.

## Result

Node v26.8.2 on `linux-x86_64`: 106,367 tests pass, 0 fail (compiler 43,656, conformance 46,182, fourslash 6,322,
fourslash-server 488, project 2,528, transpile 37, unit tests 7,154 in 36 suite groups) plus 79 eslint-rule tests.
Wall time on a loaded 24-core host: about 6 minutes in total. No engine change: the default environment and test262
are untouched.
