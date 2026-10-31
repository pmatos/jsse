# Node host mode: TypeScript's project runner

Status: accepted, epic #1214, issue #1286 (extends `2026-10-28-1900-node-host-compiler-runner.md`).

## Context

`ProjectRunner` (`src/testRunner/projectsRunner.ts`) turns each `tests/cases/project/*.json` into two mocha
`describe`s (`@module: commonjs` and `@module: amd`) of four tests: module resolution trace, errors, JS output and
declarations. A test compiles a small project of `tests/cases/projects` with the options of its json file (`outFile`,
`outDir`, `sourceMap`, `mapRoot`, `sourceRoot`, `declaration`, `resolveMapRoot`, `resolveSourceRoot`) on an in-memory
file system mounted over the real `tests` directory, and compares the trace, the errors, every emitted `.js`,
`.js.map` and `.d.ts` and the errors of compiling the emitted declarations with `tests/baselines/reference/project`.
`ts-suite/oracle.json` pins 2528 passing project tests (316 files). The `out*`, `sourcemap*`, `mapRoot*`/`maprootUrl*`
and `sourceRoot*`/`sourcerootUrl*` files are the generated matrix: 228 files, 1824 tests (absolute, relative and URL
roots, with and without `outFile` and `outDir`).

## Result

All 1824 matrix tests pass on Node and on `jsse --node`, with no engine change: the source map generator (VLQ,
relative path computation between map, source and output files, URL and absolute roots) runs identically on both. The
known-failure list is empty. The failing path is exercised by a control run: five suites run against a private
reference tree with one file appended to, rewritten or removed (`<name>.json`, `.js`, `.js.map`, `.d.ts`); both
engines must fail the same tests with the same `New baseline created` messages and write the same local files.

## Oracle

`scripts/run-ts-project-oracle.py` reuses the compiler oracle's private working directory, mocha invocation,
per-test and local-baseline comparison, ratchet and known-failure format. The project-specific part is the suite
model (a suite is one json file in one module kind, found from the title `... testcase <path>.json (@module: ...)`),
the slice table (`matrix` is the issue selection, with the four families and `all` as rows; `files:`, `match:`,
`prefix:`, `first:`, `shard:` as in the other oracles) in `scripts/ts_project_slices.py`, and the control
(`scripts/ts_project_control.py`). A batch is one mocha process per engine (60 suites by default); a project test
costs milliseconds once the 13 MB runner bundle is loaded, so the whole matrix takes about six minutes of jsse
process time.

## Alternatives

Running `hereby runtests --runner=project` against the checkout would write into `tests/baselines/local` of the
shared checkout and give totals only; the oracle keeps the private directory and compares every test and file.
