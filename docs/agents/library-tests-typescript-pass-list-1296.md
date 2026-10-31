# Library Tests: TypeScript suite pass list, ratchet and regression gate

Split from [library-tests-typescript.md](library-tests-typescript.md) to keep each file under the 20,000-byte budget.

**What it is** (#1296): a checked-in pass list for the jsse run of TypeScript's own test suite, a gate script that diffs a run against it and a ratchet hook so the list only gains entries. It is the jsse-side counterpart of `ts-suite/oracle.json` (what Node passes).

    TS_SRC_DIR=<built TypeScript v6.0.3 checkout> uv run python scripts/run-ts-pass-list.py [options] [runner[:bucket,...] ...]

Runners: `compiler conformance project transpile fourslash fourslash-server unittests` (default all; the eslint rule tests are a hereby task, not a mocha runner, and are not covered). Skips without `TS_SRC_DIR` (`--require` makes that exit 3). Never writes into the checkout (private directories of symlinks, like `scripts/run-ts-compiler-oracle.py`, whose `prepare`/`mocha_run` it reuses).

**Output and exit code.** Like `scripts/run-test262.py`: the baseline is read from `--baseline-ref` (default `origin/main`, falling back to the working tree while the file is not on main), a table per runner prints `baseline / pass / fail / new-pass / regression`, the failing baseline tests are listed (`REGRESSED: <id>`), then a `JSON:` summary line and `pass list: PASS|FAIL`. A test is a regression when the pass list holds it and it failed or did not report; new-pass when it passes and the pass list does not hold it. Exit 1 on a regression, a batch without a report (crash, timeout), a universe or recorded bucket that differs from the pin, or an empty selection; 0 otherwise. A partial run (`runner:bucket`, `--sample RATE --seed N`) only judges what it ran, so it never flags other tests.

## The pass list

A 106k-id list is 21.8 MB, which neither the token budget (20,000 bytes per authored file) nor `check-added-large-files` allows, so the list is stored as a *universe digest* and a *difference*:

- Test ids are those of `ts-suite/oracle.json` (`mocha parent full title :: test title`, `scripts/run-ts-suite-oracle.py` rules reused for the runner, bucket and sha256). The universe of a runner is listed by a Node mocha dry run (`--dry-run`, seconds); its sha256 per runner is checked against `ts-suite/oracle.json` on every run (they agree for all seven runners: 43656, 46182, 2528, 37, 6322, 488, 7154 ids), so the pass list is for exactly the tests Node passes.
- `ts-suite/jsse-pass-1296.json` records **buckets** a gate run found passing on jsse (`runner -> bucket -> [tests, first 128 bits of the sha256 of the bucket's ids]`, one line per bucket, plus the TypeScript pin and the jsse commit of the last recording). A bucket is the first letter of the file name (compiler, fourslash, fourslash-server), the directory under `tests/cases/conformance`, the unit-test suite (`unittests:: <suite>`), the file-name family (project: `map`, `maproot`, `out`, `sourcemap`, ...) or `all` (transpile). `--list` prints them, `*` marks the recorded ones.
- `ts-suite/pass-list-1296-not-passing.txt` (`runner TAB bucket TAB id TAB issue`) lists the tests of a recorded bucket that Node passes and jsse fails. The jsse pass list is the universe of the recorded buckets minus these.

A recorded bucket whose ids no longer match the checkout fails the run (the baseline is for another test set).

## Recording

    uv run python scripts/run-ts-pass-list.py compiler:A,B --update-baseline [--issue '#1418']

records every bucket the run covered completely, without a failure (`--sample` and partial buckets are never recorded), and drops the not-passing entries that now pass. A completed bucket with failing tests is only recorded with `--issue` (the issue tracking them, written to the not-passing file); otherwise it is reported and left out. It never drops an entry and refuses to touch the files when the run has a regression or a failed batch. It rewrites the working-tree files; commit them.

## Ratchet

`scripts/check_ts_pass_list_ratchet.py` (pure git-blob inspection, standard library only, no checkout and no engine): a recorded bucket may not be removed or change its size or digest, the not-passing set may only shrink (a bucket recorded by the same change may bring its known failures), the TypeScript pin may not change (a pin change needs a re-recorded baseline and a human ruling), a not-passing entry needs a recorded bucket, and the files must parse. New files are fine.

- pre-commit hook `ts-pass-list-ratchet` (`.pre-commit-config.yaml`): the index against `HEAD`, when either file is staged.
- CI: `.github/workflows/ts-suite-pass-list.yml` runs it on pull requests touching the files, as `--base <base sha> --head <head sha>` against the merge base (not a required check; the required `build` job runs the `scripts/test_*.py` suite, which covers the check on temporary git repositories). Bypassing the hook therefore cannot merge a loss silently.

Guards: `scripts/test_ts_passlist.py` (ids, buckets, selection, batching, grep, comparison, storage, ratchet rules), `scripts/test_check_ts_pass_list_ratchet.py` (the hook on temporary repositories, CI mode, the checked-in files), `scripts/test_run_ts_pass_list.py` (the gate end to end with a fake mocha: regression, crash, universe mismatch, recording with and without `--issue`, selection; needs `node`), `tests/node_ts_pass_list.rs` (`--check-universe` and the transpile runner on the real checkout; skips without `TS_SRC_DIR`).

## Cross-check against Node

`--engine node` runs the same selection of the mocha runners on Node: it must report 0 regressions and 0 failures for every recorded bucket (Node passes the whole suite); the scripted oracles compare jsse with Node themselves and have no Node-only run (`--engine node` leaves them out, naming one skips). Together with the universe digests above (the Node dry-run ids hash to the `ts-suite/oracle.json` digests for all seven runners) this is the diff against Node; the per-slice oracles (`scripts/run-ts-*-oracle.py`) keep comparing the two engines error message by error message and baseline file by baseline file, which the mocha part of this gate does not repeat (it records the jsse side only). Measured with `--sample 0.01 --seed 7` over the recorded list (1040 tests held, 1052 run): jsse 0 regressions, 0 failures; Node 0 regressions, 0 failures on the mocha runners.

## Options

`--engine {jsse,node}`, `--jsse PATH` (else `$JSSE`, else `target/release/jsse`), `--jobs N` (at most 4), `--batch-size N` (tests per mocha process, default 240; suites are never split), `--test-timeout MS` (default 600000), `--timeout SECONDS` (per process, default 7200), `--sample RATE --seed N`, `--baseline-ref REF`, `--check-universe` (dry runs only), `--list`, `--require`, `--update-baseline [--issue TEXT]`, `--keep-work DIR`, `--verbose`, `--data-dir`/`--oracle-json` (alternative files, used by the tests).

## Scripted oracles

`tsc-projects` (367 cases: `scripts/tsc-fixtures/cases*.txt`, emit matrix, `--build`), `tsc-watch` (38 sessions) and `hereby` (32 cases) are pseudo-runners of the same gate (`scripts/ts_passlist_scripted.py`, one bucket `all`, id `runner :: case`): a case passes when its oracle (`run-tsc-projects-oracle.py`, `run-tsc-watch-oracle.py --settle 3`, `run-hereby-oracle.py`) prints `ok`/`PASS`, i.e. jsse and Node agree on exit status, output and every written file. They run without `--no-node` on purpose: the digests pinned in `scripts/tsc-fixtures` depend on where the typescript package sits relative to the work directory (18 cases of the 367 differ from the pins on Node itself with another layout), Node versus jsse does not. The tsc ones need `TSC_DIR` (an installed typescript 6.0.3 package, e.g. `npm install --prefix DIR typescript@6.0.3 --ignore-scripts`, then `DIR/node_modules/typescript`), the hereby one `TS_SRC_DIR`; without `TSC_DIR` the default run leaves the two tsc runners out and naming one skips. They run after the mocha batches with the gate's `--jobs`.

## Recorded baseline (jsse built from 4eee3c40, 4 jobs, `--batch-size 240`)

| runner | tests | recorded |
|---|---|---|
| compiler | 43656 | 43074 (buckets 2, A-K, M-Y; **not L**, see below) |
| conformance | 46182 | 46182 (53 buckets) |
| project | 2528 | 2528 |
| transpile | 37 | 37 |
| fourslash | 6322 | 6322 |
| fourslash-server | 488 | 488 |
| unittests | 7154 | 7154 (36 suites) |
| tsc-projects, tsc-watch, hereby | 367, 38, 32 | 437 |

106222 tests recorded, none failing on jsse (`ts-suite/pass-list-1296-not-passing.txt` is empty). Wall time of the recording on a shared 24-core host at 4 jobs: compiler 2.5 h, conformance 2 h, unittests 0.5 h, fourslash 15 min, fourslash-server about 25 min (tsserver logs), project 3 min, transpile 6 s; about 5.5 h in all. A gate run over a sample (`--sample 0.01 --seed 7`, all runners) finishes in minutes.

**Not recorded: compiler bucket L (582 tests).** `largeControlFlowGraph.ts` fails all six of its tests with `RangeError: Maximum call stack size exceeded` in the `before all` hook (TypeScript's flow analysis recurses to `flowDepth` 2000, more than the 4000 nested JS calls of the release soft limit; Node reaches 12,523). The engine fix is PR #1448 (#1271, call-depth limits raised to 8000/10000). Once it is merged, `uv run python scripts/run-ts-pass-list.py compiler:L --update-baseline` records the bucket; recording it earlier would put a test into the pass list that `main` fails. The other 576 tests of L pass on jsse. One `unittests` batch and one `(root)` conformance bucket were re-run after fixing the gate itself (a title with a newline cut mocha's `--grep`, `conformance:(root)` is missing from `ts-suite/oracle.json`'s bucket table).

The eslint rule tests (`hereby run-eslint-rules-tests`, 79) are not a mocha runner and are not in the list.
