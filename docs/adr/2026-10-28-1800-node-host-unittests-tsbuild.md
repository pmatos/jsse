# Node host mode: TypeScript's tsbuild unit tests

Status: accepted, epic #1214, issue #1264 (extends `2026-10-28-1600-node-host-unittests-core.md` and
`2026-10-28-1700-node-host-unittests-services.md`).

## Context

`src/testRunner/unittests/tsbuild/` holds the solution builder's scenarios: 31 files register 410 mocha tests on
TypeScript v6.0.3 (the `tsbuild` suite of `ts-suite/oracle.json` pins the same 410). They run `tsc -b` (`ts.executeCommandLine`
with `--build`) and the `SolutionBuilder` API over a virtual file system and a fake host, so they are CPU-bound and
touch no real file: project references, `outFile`/`amd` bundles, `composite` and incremental `.tsbuildinfo`
(edits, touches, `--force`, `--dry`, `--clean`, `--verbose`), transitive references, `noEmit`, `noEmitOnError`,
`noCheck`, `emitDeclarationOnly`, `resolveJsonModule`, `rootDir` in a parent, module resolution across references,
declaration emit, `roots`, `fileDelete`, `extends`, library resolution, `javascriptProjectEmit`, and a public API
test with custom transformers. Each scenario prints the build output and the emitted files and compares them with the
baselines under `tests/baselines/reference/tsbuild`.

## Result

Nothing was missing in the host or the engine: every test that passes on Node passes on `jsse --node`, in light
mode and in full mode, with the default bytecode VM (410/410 on both engines). There is no defect to fix, no bug to
file, and the known-failure list (`ts-suite/unittests-core-known-failures.txt`) stays empty. No engine or host source
changed, so the default (non-`--node`) global environment and the test262 baseline are untouched.

## Oracle

The 31 groups are rows of `scripts/ts_unittests_tsbuild_groups.py`, appended to the table of
`scripts/ts_unittests_groups.py` and run by the same `scripts/run-ts-unittests-oracle.py` (per-test comparison of Node
and jsse reports, the ratchet on known failures, 600 s mocha `-t`): `run-ts-unittests-oracle.py 'tsbuild-*'`. The
four batches (20 to 23) are balanced by measured jsse time rather than test count (`declarationEmit`,
`moduleResolution`, `outputPaths` and `rootDirInParent` cost several times more per test than `noEmit`); a run takes
about five minutes at 4 jobs. `check_groups` now also
walks `unittests/tsbuild/`, so a new or renamed tsbuild suite that no group selects fails the run; `tsbuildWatch`
(`unittests/tsbuildWatch/`) stays a separate directory and ticket, and no pattern selects it. The `DESCRIBE` scan
reads a title up to its closing quote of the same kind, because `sample.ts`'s title contains `'sample1'`.

## Alternatives

* A separate script: the batching, report split and ratchet are shared, so rows were added to the table; they sit in
  their own file to keep each authored file under the 20,000-byte budget.
* One group per `describe`: the file is the unit the issue uses; titles are still compared one by one, so the finer
  split would only add rows.
