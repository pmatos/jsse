# Node host mode: tsc informational flags and tracing

Status: accepted, epic #1214, issue #1247 (builds on the project oracle of
`2026-10-27-0900-node-host-tsc-projects.md`; also settles the memory and timing
criterion of #1227).

## Decision

1. **Cases in their own file.** `scripts/tsc-fixtures/cases-info.txt`, pinned in
   `expected-info.txt`, over two new project trees: `projects/info` (a `src` tree with a
   JSON import, a plain package, a package with an `exports` map, an `@types` package, an
   `extends` chain with comments and trailing commas, a `node16` tree with `.mts`/`.cts`
   sources, a file with two errors) and `projects/info-init` (one source, no tsconfig).
   Everything compiles with `--noLib` except one case that loads `lib.es5.d.ts`; the whole
   file takes about 10 s with `--jobs 12`.
2. **Cases (24).** `--listFiles`, `--listFilesOnly` and `--listEmittedFiles` (emit,
   `--noEmit`, declaration and source maps, `-p`), with and without errors;
   `--explainFiles` (including the `node16` tree); `--showConfig` (`extends`, overrides,
   `--lib`/`--types` lists, a missing config, a file argument); `--init` (before and after
   a config exists, with options, with a bad option value, with an input file);
   `--traceResolution` under the default resolution, `paths`, `node16`
   conditions (`import` against `require`), `classic`; `--diagnostics` and
   `--extendedDiagnostics` (emit, `--noEmit`, with errors, with a real `lib`);
   `--generateTrace` (relative and nested directory); `--generateCpuProfile` (alone, with
   `--extendedDiagnostics`, with errors). A `tsconfig.json` next to command-line files is
   TS5112; the cases that name files pass `--ignoreConfig`, and one case keeps the error.
3. **Harness changes (added lines).**
   - `@stats` (leading invocation token) normalises the statistics block
     (`scripts/tsc_oracle_norm.py`): times become `<TIME>`, memory `<MEM>` and the padding
     that depends on the widest value collapses. Labels, their order and every count
     (`Files`, `Lines of ...`, `Identifiers`, `Symbols`, `Types`, `Instantiations`, the
     cache sizes) stay compared; those are deterministic and are identical on both engines.
   - `trace*.json` keeps the metadata and begin/end events (name, category, arguments)
     without `ts`; complete `X` events and `dur` go, because tsc writes an `X` event only
     for a span longer than its 10 ms sampling interval, so a slower engine writes more.
     `types.json` is compared byte for byte.
   - `*.cpuprofile` is compared by shape (below).
   - The part of the work path above the tree is `<WORK>` and `<ROOT>`, because
     `--traceResolution` prints the ancestors of the project. The work directory is
     created under `/tmp` (when that is a real, writable directory) so the number of ancestors does
     not depend on `TMPDIR`; the pins also pass with `TSC_DIR` at another depth.
4. **Engine divergences found.**
   - `global` was not defined under `--node` (#1372): tsc's `getMemoryUsage` reads
     `global.gc`, so every `--diagnostics` and `--extendedDiagnostics` run crashed after the
     compile. Fixed in `scripts/node-process-shim.js` (own commit, `tests/node_global.rs`).
   - `require("inspector")` threw `MODULE_NOT_FOUND` although `module.builtinModules` lists
     it (#1373); tsc's `enableCPUProfiler` requires it without a guard, so
     `--generateCpuProfile` crashed. `node-lib/inspector.js` adds the module.
5. **What `tsc` does without an inspector.** `enableCPUProfiler` calls `require("inspector")`
   unguarded: a throwing `require` aborts the run with a stack trace and exit 1 (what jsse
   did before). It guards only a module without `Session` (`cb()` and no profile). Neither
   writes the `.cpuprofile` Node writes, and the oracle compares every created file, so
   the module has a `Session` instead: `connect`, `post`, `disconnect`, with `post`
   answering synchronously like Node's in-thread session.
   - `Profiler.enable`, `.disable`, `.setSamplingInterval`, `.start`, `.stop` work; start
     before enable is `-32000 Profiler is not enabled` and stop before start is `-32000 No
     recording profiles found`, as in V8. Any other method calls back with
     `ERR_INSPECTOR_COMMAND` (`-32601 'X' wasn't found`); argument validation, a missing
     connection and a second `connect` use Node's codes and messages.
   - **jsse has no sampling profiler.** `Profiler.stop` returns a valid profile with a
     root and a `(program)` node, `samples: []`, `timeDeltas: []` and real `startTime` and
     `endTime`. tsc writes it (and creates the directory) exactly as it would a sampled
     one; DevTools opens it empty. `inspector.open()` throws
     `ERR_INSPECTOR_NOT_AVAILABLE`, `waitForDebugger()` `ERR_INSPECTOR_NOT_ACTIVE`,
     `connectToMainThread()` `ERR_INSPECTOR_NOT_WORKER`, `url()` is `undefined`, `close()` does
     nothing. `inspector/promises` has the promise-returning `Session`. `console`, `Network`,
     `NetworkResources` and `DOMStorage` are not provided.
   - Once a session is active tsc turns on `perf_hooks.performance` marks and measures
     (`cpuProfilingEnabled`); the `info-cpuprofile-stats` case runs that path with
     `--extendedDiagnostics`.
   - The oracle compares a profile by shape: the top-level keys, the keys every node has,
     the keys of every call frame, the root name, unique ids that `children` and `samples`
     resolve to, numeric hit counts, equal `samples` and `timeDeltas` lengths with
     non-negative deltas, and ordered times. Sample counts and node counts are not compared;
     that is the one place the comparison is weaker than byte for byte, and it exists
     because jsse records none.
6. **Result.** All 24 cases are identical on Node v26.8.2 and jsse with TypeScript 6.0.3
   once the two fixes above are in. `Memory used` is about 13 MB on Node and 200 MB on jsse
   (`process.memoryUsage().heapUsed`); only the shape (`Memory used: NNNK`) is compared.
7. **Guards.** `tests/node_tsc_projects.rs` runs the new cases against the pins;
   `tests/node_inspector.rs` and `tests/node_global.rs` cover the two modules without
   `TSC_DIR`; `scripts/test_tsc_oracle_norm.py` (in the `unittest discover` step) proves
   each normaliser keeps what it must (a changed count, label, order, trace argument or
   malformed profile is still a difference) and that `@stats` applies per invocation.

## Running it

    TSC_DIR=/path/to/typescript-6.0.3 uv run python scripts/run-tsc-projects-oracle.py --jobs 12 $(sed -n 's/^\(info-[^ ]*\) .*/\1/p' scripts/tsc-fixtures/cases-info.txt)
