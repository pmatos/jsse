# Node host mode: TypeScript's services unit tests

Status: accepted, epic #1214, issue #1261 (extends `2026-10-28-1600-node-host-unittests-core.md`).

## Context

`src/testRunner/unittests/services/` holds the language service's own unit tests: 16 files in `services/` and
`services/extract/` register 1531 mocha tests on TypeScript v6.0.3 (the `services` suite of `ts-suite/oracle.json`
pins the same 1531): `convertToAsyncFunction` (264 tests, one program each), the `extract` refactor tests
(constants, functions, ranges, symbol walker: 265), `organizeImports` (91), `textChanges` (84), `languageService`,
`cancellableLanguageServiceOperations`, `documentRegistry`, `hostNewLineSupport`, `utilities` (19 together),
`patternMatcher` (75), `preProcessFile` (41), `colorization` (38) and `transpile` (654, `transpileModule` over
option combinations). Most of them create `Program`s over `lib.d.ts` and compare printed output with baselines in
`tests/baselines/reference`.

## Result

Nothing was missing in the host or the engine: every test that passes on Node passes on `jsse --node`, in light
mode and in full mode, with the default bytecode VM. There is no defect to fix and the known-failure list (`ts-suite/unittests-core-known-failures.txt`) stays empty.

## Oracle

The groups are rows of `scripts/ts_unittests_groups.py` (batches 4 to 7, balanced from a measured run: the
`convertToAsyncFunction` and `extract*` batches take about 9 minutes (full mode: 10 and 13) of jsse time, the other two 2 to 4
minutes, run by the same `scripts/run-ts-unittests-oracle.py` with the same per-test comparison, ratchet and 600 s
mocha `-t`. `check_groups` now walks `unittests/`, `unittests/services/` and `unittests/services/extract/`, so a new
or renamed services suite that no group selects fails the run. The other subdirectories keep their own oracles
(`config`, `sys`: `run-ts-unittests-config-sys-oracle.py`).

## Alternatives

* A second script for services: the batching, the JSON-report split and the ratchet are the same, so a table was
  added instead of a fork.
* One group per describe block of `organizeImports` or `transpile`: the file is the unit the issue and the
  suite names use; per-test titles are still compared.
