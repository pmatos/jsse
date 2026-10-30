# Node host mode: TypeScript's transpile runner

Status: accepted, epic #1214, issue #1287 (extends `2026-10-28-1900-node-host-compiler-runner.md` and
`2026-10-29-0100-node-host-fourslash-runner.md`).

## Context

`TranspileRunner` (`src/testRunner/transpileRunner.ts`) turns each file of `tests/cases/transpile` into one mocha
`describe` (`transpile tests <path>`). `TranspileTestCase` multiplies a file by its `declarationMap`, `sourceMap` and
`inlineSourceMap` settings and registers up to two tests per configuration: the output of `ts.transpileModule`
(`has expected js output`, except with `@emitDeclarationOnly`) and, with `@declaration`, of `ts.transpileDeclaration`
(`has expected declaration output`). The baseline text holds the inputs, the output, the source map and the
diagnostics (`reportDiagnostics`), and is compared with `tests/baselines/reference/transpile/<name>.js|.d.ts`.
`ts-suite/oracle.json` pins 37 passing tests in 22 files. Unlike the compiler and fourslash runners nothing creates a
`Program` or reads lib files: the code path is the scanner, parser, transformers and printer on one file
(`isolatedModules` behaviour), plus the isolated-declarations emitter.

## Result

All 37 tests pass on `jsse --node` as on Node, and the local baseline directory stays empty on both. No engine or
host change was needed, so the default global environment is untouched and the test262 pass list is unaffected.
`ts-suite/transpile-known-failures.txt` is empty. A control run (three corrupted reference baselines: appended,
rewritten, removed; two inputs changed so the output differs from the reference) fails the same six tests on both
engines with identical messages and identical `New baseline created` files, which exercises the failing and
baseline-writing path.

Because 22 inputs say little about the isolated path, `--corpus` runs `transpileModule` and `transpileDeclaration`
over the test files of `tests/cases/{transpile,compiler,conformance}` with seven option sets and compares the digest
of text, source map and diagnostics per case between the engines. Over all 12,466 files (87,262 cases) the engines agree, with one class of exception:
`binderBinaryExpressionStress.ts` (and its `Js` twin) overflow the native stack in `transpileDeclaration` on jsse and
in a cold Node process, but not in a Node process that has already optimised the visitor. The stack depth limit is
implementation-defined (V8's is in bytes), so such cases are reported and not compared; they are not an engine bug
to file. The tree-walker (`--no-bytecode`) agrees on a 1,500-file corpus run.

## Oracle

`scripts/run-ts-transpile-oracle.py` reuses the compiler oracle as the fourslash oracle does (working directory,
mocha invocation, per-test and local-baseline comparison, ratchet and the known-failure file format). The
transpile-specific parts are small modules: `ts_transpile_slices.py` (file grouping by describe title, slice table,
pinned count only for a whole-runner run, because `oracle.json` has no name buckets for this runner),
`ts_transpile_control.py` and `ts_transpile_corpus.py` with its driver `ts-transpile-corpus.js`.

## Alternatives

* Run the corpus through the mocha harness: it would need a baseline per case; a digest per case needs none and the
  two engines are each other's reference.
* Fold the corpus into `run-ts-compiler-oracle.py`: that file is near its size budget and the corpus has no mocha
  report to share.
