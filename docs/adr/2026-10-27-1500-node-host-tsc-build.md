# Node host mode: tsc `--build` and incremental scenarios

Status: accepted, epic #1214, issue #1246 (builds on the project oracle of
`2026-10-27-0900-node-host-tsc-projects.md`).

## Decision

1. **Own cases file.** The `--build`, `--incremental`, `--clean`, `--force`,
   `--dry` and `.tsbuildinfo` cases live in `scripts/tsc-fixtures/cases-build.txt`,
   pinned in `expected-build.txt`, on the project tree `projects/chain`. The harness
   (`scripts/run-tsc-projects-oracle.py`) now loads `cases.txt` plus every
   `cases-<topic>.txt`; a topic file pins to `expected-<topic>.txt`, `--pin`
   writes each pin file from its own cases, case names are unique across files.
   Nothing else about the harness changed.
2. **Scenario steps.** A step of a case that starts with `@` is run by the
   harness between tsc invocations, identically for both engines:
   `@utimes EPOCH PATH..` (a directory: everything below it), `@replace PATH OLD
   NEW` (first occurrence; a missing text aborts), `@remove PATH..`, and
   `@mtimes-after EPOCH PATH..` (prints `after`/`not after` into the case's
   stdout, so the diff proves that tsc's own `utimes` calls took effect on jsse).
   Steps record no exit code: the exit-code field counts only tsc invocations.
3. **Deterministic mtimes.** tsc decides up-to-dateness by comparing the mtimes
   of inputs, outputs and the `.tsbuildinfo`. Cases therefore never rely on the
   wall clock: inputs start at epoch 1.6e9, before every touch everything is reset
   to 1.7e9, and a "touch" sets one input to 1.75e9. All three are in the past, so
   the next build's real "now" is always newer than them, on any runner speed. A
   case never sleeps.
4. **Project.** `chain` is `core <- util <- app` plus an independent `solo`, all
   composite with `declarationMap` and `sourceMap`, and a `files: []` solution
   `tsconfig.json`. `--lib es2020` keeps each compile at 1 to 10 s on jsse (the
   default `lib.d.ts` set costs 60 to 110 s).
5. **Cases (14).** `build-chain` (fresh build, up to date, body-only change in
   `core`: `util` and `app` only get "up to date with .d.ts files" timestamp
   updates and `solo` is untouched; then a `.d.ts` change: `util` rebuilds, `app`
   does not), `build-touch-only` (a bare `utimes` rebuilds nothing, outputs get
   new timestamps), `build-dry-force`, `build-clean`, `build-subproject` (`-b util`,
   `-b solo app`), `build-errors` (`util` has an error, `app` is skipped, fixing
   it and rebuilding), `build-stop-on-errors`, `build-bad-project`
   (missing project, a file instead of a config, `--clean --verbose` error),
   `build-missing-buildinfo`, `build-stale-buildinfo` (version field mismatch),
   `build-removed-input`, `incremental-project` (`-p core` twice, edit, twice),
   `incremental-noemit`, `incremental-errors` (semantic diagnostics stored in
   the buildinfo and replayed).
6. **What is compared.** As in the project oracle: stdout and stderr per
   invocation, exit codes, and every created, changed or deleted file, which
   includes every `.tsbuildinfo` (file hashes are tsc's sha256 over
   `crypto.createHash`, so a hash mismatch shows up as a byte difference). The
   buildinfo stores the TypeScript lib paths relative to the project; the node
   and jsse trees sit at the same depth so those strings match once `<TSC>` is
   normalised, and the work-tree path never appears in them.
   That relative path still depends on where `TSC_DIR` sits relative to the work
   tree (`$TMPDIR/tsc-oracle-*/<engine>/<case>`), so the pins, like the older
   `incremental` pin, only match on the layout they were taken on; node-vs-jsse runs
   are unaffected. Normalising the path would re-pin them and needs a ruling.
7. **Result.** All 14 cases are identical on Node v26.8.2 and jsse with
   TypeScript 6.0.3; the acceptance criteria (byte-identical buildinfo, second
   run up to date, a touch rebuilds only the dependents) hold, with no engine
   change. Wall time with a non-LTO release build on a loaded host: Node 0.5 to
   3 s per case, jsse 2 to 50 s; the 14 cases take about 1.5 minutes with
   `--jobs 8`.
8. **Guards.** `tests/node_tsc_projects.rs` already runs every case file with
   `--no-node` against the pins (it needs `TSC_DIR`; CI has none until #1250).
   `scripts/test_run_tsc_build_scenarios.py` runs in CI: it drives the harness
   with fake engines to prove the scenario steps and the topic pin files, and
   checks that `cases-build.txt` and `expected-build.txt` agree.

## Running it

    TSC_DIR=/path/to/typescript-6.0.3 uv run python scripts/run-tsc-projects-oracle.py \
        $(grep -oE '^(build|incremental)[a-z-]*' scripts/tsc-fixtures/cases-build.txt)
