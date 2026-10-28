# Node host mode: TypeScript's top-level unit tests

Status: accepted, epic #1214, issue #1259 (builds on `2026-10-27-1600-node-host-mocha.md`).

## Context

`src/testRunner/unittests/*.ts` holds the compiler's own API tests (asserts, builder, canWatch, incrementalParser,
JSDoc parsing, module resolution, printer, program and public API, program reuse, semver, transforms, the regular
expression scanner, ...): 26 files, 2334 mocha tests on TypeScript v6.0.3. They run in `built/local/run.js` with
the `unittest` runner, the way `hereby runtests --runner=unittest` starts it. Subdirectories (`config`, `evaluation`,
`services`, `sys`, `tsbuild`, `tscWatch`, `tsserver`) have their own tickets.

## Result

With the host work of the earlier tickets nothing was missing: every test that passes on Node passes on jsse
`--node`, in light mode (hereby's default) and in full mode (`--no-light`, the Node oracle's setting, which also
asserts parse-tree invariants on every parsed file), with the default bytecode VM. There is no engine, host or
harness defect to fix and the known-failure list (`ts-suite/unittests-core-known-failures.txt`) is empty.

The one thing that is not like Node is time. A jsse process spends about 80 s loading and running the 13 MB
runner bundle, and the first `Program` a suite creates parses `lib.d.ts` and the DOM lib (2.5 MB of TypeScript):
0.15 s on Node, 9 s in a plain script on jsse, 25 s (light) to 75 s (full) inside the runner, over hereby's 40 s
per-test limit. That is interpreter speed, not a conformance difference, so the oracle passes mocha a longer
`-t` (`--test-timeout`, 600000 ms) on both engines instead of weakening or skipping the tests; the wall time of
the suite is the business of #1297. `canWatch` (92 tests, 1.5 s each on jsse) and `programApi` take most of the test
time.

## Oracle

`scripts/run-ts-unittests-oracle.py` (groups in `scripts/ts_unittests_groups.py`) runs one mocha process per engine
per batch of groups with the JSON reporter and splits the report by the groups' `-g` patterns, so the comparison
is per test title and multiplicity. It reuses the working-directory layout of `run-ts-mocha-oracle.py` (symlinks
into the checkout, private `tests/baselines/local`, so nothing is written into it), at most four processes at once, and
kills a process group on timeout. It fails when Node does not pass the pinned count of a group, when a test is
missing on either side, when a jsse failure is not in the known-failure list, when a listed failure passes, and
when a top-level `describe` of the checkout belongs to no group (or to two).

## Alternatives

* Diff mocha's whole stdout like the `ts-*` cases of the mocha oracle: tests print durations and stacks that
  differ per engine; the per-test sets are what the acceptance criterion states.
* Run the groups through `hereby runtests`: it adds the `failed-tests` reporter and no JSON; the runner flags are
  already covered by the `cli-*` cases.
