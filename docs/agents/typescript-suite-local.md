# Running tsc and the TypeScript suite on jsse locally

Epic #1214 runs TypeScript 6.0.3 (`tsc`, `tsserver`, the language service and TypeScript's own test suite) on `jsse --node` and checks it against Node. This page is the map: what to install, which script runs what, what it costs, and which CI jobs exist. Architecture: [node-host.md](node-host.md). When a run fails: [typescript-suite-triage.md](typescript-suite-triage.md). Every per-oracle detail page is indexed in [library-tests-typescript.md](library-tests-typescript.md).

## Ingredients

| need | how | used by |
| --- | --- | --- |
| a jsse binary | `cargo build --release --target-dir <private dir>` (the shipped profile, fat LTO, slow to link). For iteration `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo build --release --target-dir <dir>` takes about 2 minutes (never compare timings across the two). Pass it as `--jsse BIN` or `JSSE=BIN`; the default is `target/release/jsse` | everything |
| Node 26.8.2 | the oracle engine recorded in `ts-suite/oracle.json` (`node --version`) | every Node side, `uv`, `npm` |
| `TSC_DIR`, an installed `typescript` 6.0.3 package | `npm install --prefix <dir> typescript@6.0.3 --ignore-scripts`, then `TSC_DIR=<dir>/node_modules/typescript` | the `tsc` oracles (`lib/_tsc.js`, `bin/tsc`) |
| `TS_SRC_DIR`, the pinned TypeScript checkout, built | `uv run python scripts/run-ts-suite-oracle.py --dir <dir> fetch install build`, then `TS_SRC_DIR=<dir>/ts-src` (clone of `v6.0.3`, `npm ci`, `hereby local` and `hereby tests`; the pin is `ts-suite/pin.json`). Without `--dir` the cache is `$TS_SUITE_DIR` or `~/.cache/jsse-ts-suite`, which already holds a built tree on the shared development host | hereby, mocha, every `run-ts-*` oracle |

The `run-ts-*` and `run-hereby-*` oracles never write into `TS_SRC_DIR` or `TSC_DIR` (they work in private directories of symlinks), so one built tree can serve every session; only `run-ts-suite-oracle.py` builds, and only inside its cache directory. Without its variable an oracle prints `SKIP` and exits 0 (`--require` turns that into an error). Nothing from TypeScript is vendored in this repository.

To run the compiler by hand: `jsse --node $TSC_DIR/bin/tsc --noEmit --strict a.ts` (or `lib/_tsc.js`); `--version` takes about a second, the first compile against the default `lib.d.ts` 15 to 20 seconds on a loaded host.

## Which script runs what

All commands run from the repository root. Cost figures are wall time at 4 jobs on the shared 24-core host unless noted.

| question | command | cost |
| --- | --- | --- |
| does `tsc` start and behave like Node? | `TSC_DIR=... ./scripts/run-tsc-oracle.sh [--full] [--no-timing]` | seconds |
| do projects, emit options, diagnostics, `--build`, informational flags match Node? | `TSC_DIR=... uv run python scripts/run-tsc-projects-oracle.py [--jobs N] [--list] [case ...]` (413 cases listed by `--list`, from `scripts/tsc-fixtures/cases*.txt`) | 4 s per case on jsse; minutes in all |
| `tsc --watch` sessions | `TSC_DIR=... uv run python scripts/run-tsc-watch-oracle.py` | minutes |
| can jsse compile TypeScript with itself? | `TSC_DIR=... uv run python scripts/run-tsc-selfcompile-oracle.py --smoke` (the full run also needs `TS_SRC_DIR`) | minutes; full run is the nightly |
| hereby tasks (`hereby local`, tests, lint, stacks) | `TS_SRC_DIR=... uv run python scripts/run-hereby-oracle.py`, `run-hereby-local-oracle.py`, `run-hereby-tests-oracle.py`, `run-hereby-stacks-oracle.py`, `run-ts-lint-oracle.py` | minutes to an hour |
| mocha CLI, reporters, TypeScript's runner options | `TS_SRC_DIR=... uv run python scripts/run-ts-mocha-oracle.py` | an hour |
| one TypeScript runner against Node | `scripts/run-ts-<runner>-oracle.py [slice ...]` for `compiler`, `conformance`, `fourslash`, `project`, `transpile`, `unittests-*`; slices narrow it (`first:N`, `prefix:A-C`, `files:a.ts`, `--list`) | a slice of 200 suites: 4.5 minutes; the whole suite: about 6.6 hours |
| the whole suite as a regression gate | `TS_SRC_DIR=... uv run python scripts/run-ts-pass-list.py [runner[:bucket] ...] [--sample 0.01 --seed 7] [--list] [--check-universe]` | `transpile`: 6 s; a 1% sample: minutes; everything: about 5.5 h |
| memory and time budget | `TS_SRC_DIR=... uv run python scripts/run-ts-budget-oracle.py [--smoke]`; `--check-budget` validates the shipped file without a checkout | smoke: minutes |
| Node itself still passes the pinned suite | `uv run python scripts/run-ts-suite-oracle.py` (about 6 minutes; `check` compares with `ts-suite/oracle.json`) | 6 minutes |
| esbuild, WebAssembly | `ESBUILD_DIR=... ./scripts/run-node-esbuild-oracle.sh`, `./scripts/run-node-wasm-oracle.sh` | minutes |

The pass-list gate is the one to run before merging an engine change that could touch `tsc`: `run-ts-pass-list.py --sample 0.01 --seed 7` over every runner takes minutes and exits 1 on any regression against `ts-suite/jsse-pass-1296.json`. Output and options: [library-tests-typescript-pass-list-1296.md](library-tests-typescript-pass-list-1296.md). Oracles that compare two engines test by test (the `run-ts-*-oracle.py` family) also show *why* a test fails (error message, baseline file), which the gate does not.

Verified for this page on a LTO-off build: `run-tsc-oracle.sh --no-timing` (all 10 checks ok), `run-tsc-projects-oracle.py noemit declaration` (2/2), `run-ts-pass-list.py transpile` (37/37, `pass list: PASS`), `run-ts-pass-list.py --list`, `run-ts-budget-oracle.py --check-budget` (14 cases), `run-ts-compiler-oracle.py --list first:3`, `run-ts-suite-nightly-1298.py plan --shards 4`.

## Host rules for long runs

The development host (24 cores) is shared with other sessions.

- At most 4 jsse/node test processes at once: `--jobs 4` or fewer (the unit-test oracles reject more). A jsse process loads a 13 MB bundle for 30 s and about 1.7 GiB; four of them is the budget.
- The oracles start every child through `scripts/oracle_procs.py` (`run_group`, `group`, `call`; [API and guards](library-tests-typescript-conformance-process-groups.md)): the child leads a session, and in every exit path (normal end, timeout, SIGINT/SIGTERM/SIGHUP) the whole process group is killed, so forked mocha workers are not orphaned. Use it for your own long commands (`from oracle_procs import run_group`), or `setsid` plus `timeout`, and kill by recorded PID. Never `pkill -f` a pattern.
- Run long jobs in the background with a durable log and an exit-code file (`cmd > log 2>&1; echo $? > exit`) and poll with a loop that tests for the exit file. Do not rebuild the binary while a suite run uses it: copy the binary first.
- Before finishing, `ps -eo pid,ppid,etimes,cmd | grep -E 'jsse|node'` and kill what you started by PID.
- Use a private scratch directory and a private `--target-dir`; the shared one holds other sessions' files.

## CI

None of the TypeScript jobs is a required check; the required checks are `build`, `Validate PR title` and `SonarCloud analysis`. The `build` job runs the `scripts/test_*.py` suite (`python -m unittest discover -s scripts -p 'test_*.py'`) and `cargo nextest run --cargo-profile release-nolto` (plus the test262 smoke, sample and `test262-extra` steps), which cover the oracles' logic on fake inputs and, where `TS_SRC_DIR` or `TSC_DIR` is set, the real thing.

| workflow | what | detail |
| --- | --- | --- |
| `ts-suite-pass-list.yml` | pass-list ratchet on PRs touching the list | [pass list](library-tests-typescript-pass-list-1296.md) |
| `ts-suite-nightly-smoke.yml` | 0.2% sample of the gate, on PRs touching the engine or scripts | [nightly](library-tests-typescript-nightly-1298.md) |
| `nightly-ts-suite-1298.yml` | the whole gate, 20 shards, 01:45 UTC; non-blocking until `TS_SUITE_BLOCKING` is flipped | [nightly](library-tests-typescript-nightly-1298.md) |
| `ts-suite-budget-smoke.yml`, `nightly-ts-suite-budget.yml` | RSS cap (6x Node) and wall-time record | [budget](library-tests-typescript-budget.md) |
| `tsc-selfcompile-smoke.yml`, `nightly-tsc-selfcompile.yml` | compile TypeScript's `src/` with tsc on jsse | [oracles](library-tests-typescript-oracles.md) |

Read a nightly result with `gh run list --workflow nightly-ts-suite-1298.yml --repo pmatos/jsse --limit 3`, then `gh run view <id>`; the merged `summary.md` lists regressions and `new-pass` counts (tests that now pass and are not recorded yet: record them with `--update-baseline`, see [triage](typescript-suite-triage.md)).
