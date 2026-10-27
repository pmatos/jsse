# Node host mode: tsc project oracle

Status: accepted, epic #1214, issue #1243 (builds on #1242,
`2026-10-27-0100-node-host-tsc-startup.md`).

## Decision

1. **One script, fixed project set.** `scripts/run-tsc-projects-oracle.py` runs
   every case of `scripts/tsc-fixtures/cases.txt` through `lib/_tsc.js` on Node
   and on `jsse --node`, each in a fresh copy of a tree from
   `scripts/tsc-fixtures/projects/`, and prints a pass/fail table (seconds per
   engine, the parts that differ). It exits 1 on any difference. A case line is
   `name | project | expected exit codes | arguments`; ` ;; ` chains further
   invocations in the same tree (`--build` twice, `--build --clean`,
   `--incremental` twice). The expected exit codes are checked on both engines so
   a degenerate case cannot pass by both printing the same usage text (the first
   draft forgot the input files and "passed" on `tsc --help`).
2. **What is diffed.** Per invocation: stdout, stderr and the exit code; per
   case: every file the compiler created, changed or deleted. The only
   normalisation is the work-tree path (`<DIR>`), the TypeScript package path
   (`<TSC>`) and `h:mm:ss AM` clock stamps (`--build --verbose` prints them).
   Nothing else is rewritten, so line endings, source-map `sources`, the
   `.tsbuildinfo` contents and the ANSI code frames of `--pretty` all count.
3. **Cases.** `app` (a four-module project: abstract and generic classes, `#private`
   fields, enums, `const enum`, mapped and template-literal types, overloads,
   async/await, generators, async generators): `--noEmit`; emit as CommonJS,
   ESNext/bundler and ES2015 (generator downlevel); `--declaration
   --declarationMap --sourceMap`; `--emitDeclarationOnly`; `--incremental`
   twice; `--listFilesOnly`, `--listEmittedFiles`, `--explainFiles`,
   `--traceResolution`; and one compile against the default `lib.d.ts` set.
   `errors` (over thirty semantic diagnostics under `--strict` across three files,
   syntax errors in a fourth): plain, strict, emit with errors (exit 2),
   `--noEmitOnError` (exit 1), `--pretty`, syntax only. `ts-app` and
   `bad-config`: `-p .` with `extends`, `include`/`exclude`, `--showConfig`, an
   extending config that turns checks on, and a config with invalid values.
   `refs`: `--build --verbose` (composite projects with references), the
   up-to-date rerun, and `--clean`.
4. **Result.** All 23 cases are identical on Node v26.8.2 and jsse with
   TypeScript 6.0.3; no engine divergence surfaced, so there is no engine change
   in this PR. Wall time on a loaded 24-core host with a non-LTO release build:
   Node 0.2 to 1.8 s per case; jsse 1 to 11 s with `--lib es2020`, and 60 to 85 s
   for the two cases that load the default `lib.d.ts` set (`default-lib`,
   `tsconfig-bad`), the slow path #1242 measured. The full run takes a couple of minutes
   with `--jobs 8`.
5. **Pins.** `--pin` rewrites `scripts/tsc-fixtures/expected-projects.txt`
   (sha256 of each case's normalised Node result; two consecutive pins were
   byte-identical). `--no-node` compares jsse against those digests, and
   `tests/node_tsc_projects.rs` does so through the built binary.
6. **Skips and CI.** Without `TSC_DIR`, or with another TypeScript version, the
   script prints `SKIP: ...` and exits 0; `--require` turns a skip into exit 3.
   `--ci` prints plain output, full diffs and `::error` annotations. The sources
   are still not vendored (#1250), so the repository's CI never has `TSC_DIR`
   and these runs are local until that lands. What CI does run is
   `scripts/test_run_tsc_projects_oracle.py` (in the `unittest discover` step):
   it drives the harness with a fake typescript package and fake `node`/`jsse`
   executables to prove that path and clock differences are ignored and that a
   stdout, stderr, emitted-file, exit-code or wrong-expectation difference fails,
   and it checks that `cases.txt` and the pins agree.

## Running it

    TSC_DIR=/path/to/typescript-6.0.3 uv run python scripts/run-tsc-projects-oracle.py [--jobs N] [--ci] [case ...]
