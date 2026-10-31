# Library Tests: nightly TypeScript suite on jsse (#1298)

Split from [library-tests-typescript.md](library-tests-typescript.md) to keep each file under the 20,000-byte budget.

The nightly job runs TypeScript 6.0.3's own test suite on `jsse --node` with the gate of [#1296](library-tests-typescript-pass-list-1296.md) (`scripts/run-ts-pass-list.py`, which diffs a run against `ts-suite/jsse-pass-1296.json`), sharded over a job matrix, and fails on baseline regressions. The sampled smoke of the same machinery runs in CI.

| file | what |
|---|---|
| `.github/workflows/nightly-ts-suite-1298.yml` | nightly (01:45 UTC), `workflow_dispatch`, and `pull_request` when the workflow file itself changes |
| `.github/workflows/ts-suite-nightly-smoke.yml` | `--sample 0.002 --seed 1` (about 210 tests, a few minutes) on pull requests and pushes touching the engine or the scripts; not a required check |
| `scripts/run-ts-suite-nightly-1298.py` | `shard`, `merge` and `plan` commands |
| `scripts/ts_nightly_1298.py` | partition, result files, merge verdict (standard library only) |
| `scripts/test_ts_nightly_1298.py` | guards: partition, merge on fake shard directories, the workflow's matrix and pins, two shards of the real gate over a fake mocha (needs `node`) |

## Jobs

1. **build**: `cargo build --release` (the shipped profile, fat LTO), uploads `target/release/jsse` as artifact `ts-suite-jsse` (kept 2 days).
2. **shard** (matrix `shard: [0 .. TS_SHARDS-1]`, `fail-fast: false`, 340 minute limit): downloads the binary (`chmod +x`, artifacts drop the mode), fetches and builds the pinned TypeScript tree (`scripts/run-ts-suite-oracle.py fetch install build`, which runs `hereby local` and `hereby tests`; no `npx`), installs `typescript@6.0.3` for the scripted `tsc` oracles (`TSC_DIR`), then runs `run-ts-suite-nightly-1298.py shard --shard N --shards $TS_SHARDS --jobs 2`. It uploads `ts-suite-shard-N` even when the gate failed. The step exits 0 whenever the gate ran and wrote its results (a regression is the report job's verdict, not the shard's); a shard job is red only when the harness did not run (build, checkout, a crash).
3. **report** (`if: !cancelled()`): downloads `ts-suite-shard-*`, runs `merge`, appends `summary.md` to the step summary and uploads `ts-suite-nightly-results`.

All actions are pinned by SHA like the other workflows; the matrix values go to the shell through `env:`.

## Blocking switch

`TS_SUITE_BLOCKING` in the workflow's top-level `env:` is the one line to change. `"false"` (now): a baseline regression (a test in the pass list that failed or did not report) and a crashed batch (mocha exited without a report) are `warning:` lines in the merge log and the step summary table (not workflow annotations), and the workflow is green. `"true"`: the report job passes `--blocking` and fails on them. Integrity problems fail the report job in both modes: a shard with no result directory, a gate that did not run, a runner missing from the universe, a universe that differs between shards, shards that together ran a different number of tests than the universe holds (a suite run twice or by nobody), a universe or recorded bucket that differs from the pin (`ts-suite/oracle.json`, `ts-suite/jsse-pass-1296.json`). A scheduled workflow is not a required check; flipping the switch makes the nightly run red, nothing else.

## Sharding

`ts_nightly_1298.partition` takes the suites the gate selected (a suite is a mocha parent title and is never split; the scripted oracles' cases are one suite each) and assigns them longest-processing-time-first to the least loaded of N shards, with `WEIGHT[runner]` seconds per test as the cost and the suite name as the tie break, so every shard derives the same assignment from the same pinned universe without coordination. `WEIGHT` comes from the recording of #1296 (4 jobs on a shared 24-core host: compiler 2.5 h for 43656 tests, conformance 2 h for 46182, unittests 0.5 h for 7154, fourslash 15 min, fourslash-server 25 min, project 3 min, scripted oracles guessed); each mocha batch (240 tests) pays about 60 s to load the 13 MB runner bundle, which is part of those per-test costs. Refresh the weights from the `timings.json` of a nightly run; a wrong weight only unbalances the shards, `merge` still proves every test ran exactly once. `TS_SHARDS` (20 = the free-plan concurrency of public repositories) and the matrix list in the workflow must agree; the unit test checks it. With `--jobs 2` (a runner has 4 vCPUs and 16 GB; the budget job uses 2 for the same reason) each shard is about 1.2 job-hours on the recording host.

## Artifacts

`ts-suite-shard-N` (14 days; a directory `results/` and `results.log`):

- `summary.json`: schema, `shard`, `shards`, `ran` (the gate got as far as its table), `exit` (the gate's exit status), `secs`, `universe` and `selected` (tests per runner), `runners` (`held`/`pass`/`fail`/`new_pass`/`regression` per runner, as the gate's table), `batch_errors` (mocha batches without a report), `integrity` (universe and bucket problems).
- `pass-<runner>.txt`, `fail-<runner>.txt` (failed, not in the pass list), `regression-<runner>.txt` (in the pass list, failed or did not report): the test ids of the shard, sorted, one per line, backslash, LF and CR escaped like `node-pass-list.txt`; a file exists when it would not be empty.
- `timings.json`: one record per mocha run or scripted-oracle run: `runner`, `tag` (`compiler-3`, `list-compiler` for the Node dry run that lists the universe, or the scripted runner), `tests` (cases run; the whole listing for a dry run), `secs`, `exit`, `dry`.

`ts-suite-nightly-results` (30 days): the same files merged over all shards (`pass-<runner>.txt` is the per-runner pass list of the night; `timings.json` records get a `shard` field), `summary.json` (per-runner and total counts, `integrity_problems`, `gating_problems`, `blocking`) and `summary.md` (the table, job time per runner, shard wall times). `new_pass` in the table counts tests that pass and are not in `ts-suite/jsse-pass-1296.json` yet: record them with `run-ts-pass-list.py ... --update-baseline` (the nightly never writes the baseline).

## Reproduce a shard locally

    TS_SRC_DIR=<built TypeScript v6.0.3 checkout> [TSC_DIR=<typescript 6.0.3 package>] \
      uv run python scripts/run-ts-suite-nightly-1298.py shard --shard 3 --shards 20 --out /tmp/shard3 --jobs 2
    uv run python scripts/run-ts-suite-nightly-1298.py merge --results /tmp --out /tmp/merged --shards 20

`--sample RATE --seed N` and `runner[:bucket]` specs narrow a run (`merge --sampled --runners ...` for the matching verdict); without `TSC_DIR` the `tsc-projects` and `tsc-watch` runners are left out of the universe and `merge` reports them missing unless `--runners` omits them. The baseline is read from `--baseline-ref` (default `HEAD`: the pass list of the checked-out commit, which a shallow CI clone has, unlike `origin/main`).

## Measurements

Sampled run (`--sample 0.002 --seed 1`, 210 tests in 49 suites, no-LTO build, 2 jobs, 24-core host): all 210 pass, 0 regressions, 138 s wall; the shard, merge and artifact layout above come from that run. The first full dispatch is recorded in the pull request.
