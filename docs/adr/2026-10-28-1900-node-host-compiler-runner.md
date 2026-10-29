# Node host mode: TypeScript's compiler runner

Status: accepted, epic #1214, issue #1268 (extends `2026-10-27-1600-node-host-mocha.md` and
`2026-10-28-1600-node-host-unittests-core.md`).

## Context

`CompilerBaselineRunner` (`src/testRunner/compilerRunner.ts`) turns each file of `tests/cases/compiler` into one
mocha `describe` (one per configuration when a header such as `// @target: es5, es2015` varies an option) with six
tests: errors, module-resolution trace, sourcemap record, JS output, sourcemap output, types and symbols. Each test
compiles the file in a virtual file system with the lib files of `built/local`, renders a baseline and compares it
with `tests/baselines/reference/<name>.<ext>`; a difference writes the actual text to `tests/baselines/local` and
fails the test. `ts-suite/oracle.json` pins 7276 suites, 43656 tests, all passing on Node. The #1257 `cli-*` cases
already prove that jsse lists the same titles; this issue runs real tests.

## Result

The first 200 suites (1200 tests, `2dArrays.ts` to `ClassDeclarationWithInvalidConstOnPropertyDeclaration2.ts`)
pass on both engines, 1200/1200. No engine or host source changed, so the default (non-`--node`) global
environment and the test262 baseline are untouched, and there is no divergence to root-cause or to list: the
known-failure file `ts-suite/compiler-known-failures.txt` is empty. Because everything passes, the baseline IO on
the failing path (the `New baseline created` error, the local file, the `.delete` marker) is exercised by a control
run: seven suites run against a deliberately corrupted private copy of their reference baselines and the engines must
fail the same tests with the same messages and write byte-identical local files (they do).

## Oracle

`scripts/run-ts-compiler-oracle.py` is the machinery #1269-#1273 reuse. A mocha dry run on Node lists the suites in
enumeration order; a slice spec (`scripts/ts_compiler_slices.py`: `first:N`, `prefix:` character ranges, `shard:I/N`,
`files:`, `match:`, named rows, unions) picks them; batches of suites (default 40) run one mocha process per engine,
at most four processes at once over both engines (host protection: every process group is killed by
`scripts/oracle_procs.py`), each in a private working directory of symlinks so the shared checkout is never written
to and each run has its own `tests/baselines/local`. Batching matters: the runner bundle takes about 80 s to load on
jsse and the first `Program` (lib.d.ts) 25 to 75 s, so cost is per process, not per test. Results are compared per
test (JSON reporter), with the error messages of tests failing on both, and with every local baseline file; the
ratchet and the pinned Node count follow the unit-test oracles. New ranges need a row in the slice table (or a module
named by `--slices-module`), not a change to the core.

## Alternatives

* One process per suite: 25 to 75 s of lib.d.ts parsing and 80 s of bundle loading each; rejected.
* `hereby runtests-parallel` (forked workers): its per-test listing exists only as `TEST-results.xml` under
  `CI=true`, and it writes into the checkout's `tests/baselines/local`; the mocha CLI per batch gives the JSON report
  and a private directory.
* Comparing only totals: a swapped failure would hide, so tests are compared by title.
