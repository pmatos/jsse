# Library Tests: `tsc` informational flags and tracing, second set (#1247)

Part of [library-tests-typescript.md](library-tests-typescript.md); the oracle is the `tsc` project oracle of
[library-tests-typescript.md](library-tests-typescript.md) (`scripts/run-tsc-projects-oracle.py`, #1243). The first set of
informational cases (`scripts/tsc-fixtures/cases-info.txt`, 24 cases, PR #1375, design in
`docs/adr/2026-10-27-1100-node-host-tsc-info.md`) covers each flag once on a flat project. This set runs the same flags
where tsc's output is shaped by more machinery.

**What it is**: `TSC_DIR=<typescript 6.0.3 package> uv run python scripts/run-tsc-projects-oracle.py [--jobs N] $(sed -n 's/^\(infm-[^ ]*\) .*/\1/p' scripts/tsc-fixtures/cases-info-more.txt)`
runs the 46 `infm-*` cases of `scripts/tsc-fixtures/cases-info-more.txt` (pinned in `expected-info-more.txt`) on Node and
`jsse --node` and compares stdout, stderr, exit status and every file the compiler wrote, exactly as for the other case files
(nothing of the case loader changed: `cases-<topic>.txt` and `expected-<topic>.txt` are picked up by name). Skips without
`TSC_DIR`. Guards: `tests/node_tsc_projects.rs` (already runs every `cases*.txt` case on jsse alone against the pins, these
included; needs `TSC_DIR`; run it with `--profile release-nolto`), `scripts/test_tsc_oracle_norm_1247.py` (case file shape,
the normalisers below), `tests/node_inspector_callback_throw.rs` (the engine fix below, no `TSC_DIR`).

**Trees.** `scripts/tsc-fixtures/projects/info-more` is a bundler-resolution project: `package.json` with `type: module`,
`imports` (`#internal/*`), `exports` (self reference `info-more/feature`), a package with `typesVersions` (`typed`), a
scoped package (`@scope/lib`), a JavaScript package (`jspkg`), `typeRoots` with a types package (`special`), `/// <reference
path|types|lib>` directives, a JSON module, a `.tsx` file, a config with `paths`, `rootDirs` and `moduleSuffixes`, one with
`allowJs`/`checkJs`/`declarationDir`/maps, a `composite` one, one with `references`, one with `outFile` and `amd`, a
`sub/tsconfig.json` with an `extends` array (a config in `node_modules` and a local one), `${configDir}` templates and
`compileOnSave`/`watchOptions`/`typeAcquisition`, a tree with a non-ASCII, space and `%` file names plus a case-mismatched
import (`tsconfig.uni.json`) and three response files. The `--build` cases reuse `projects/chain`, with `@utimes` first so
the up-to-date checks do not depend on the clock.

**Cases (46).**
- `--build` with `--listFiles`, `--listEmittedFiles`, `--explainFiles`, `--extendedDiagnostics`/`--diagnostics`,
  `--traceResolution`, `--generateTrace`, `--generateCpuProfile`, `--showConfig` and `--init` (the last two are errors in
  build mode), on a first build, an up-to-date build, `--force`, and a project with an error (`infm-b-*`).
- Project references: `--explainFiles` ("File is output of project reference source"), `--traceResolution` and
  `--extendedDiagnostics` against a built referenced project, and the same unbuilt (TS6305) (`infm-refs-*`).
- `--explainFiles` for `include`, `files`, imports with a `packageId`, `exports`/`imports` maps, `typesVersions`, `/// <reference>`
  forms, `lib` (also `--noResolve --noLib`), `paths`, `--locale de`/`ja` (`infm-explain*`).
- `--listFiles`/`--listEmittedFiles` with JavaScript sources and declaration maps, `--noEmitOnError`, `outFile`, `--emitDeclarationOnly`,
  `--noEmit`, `composite` (the `.tsbuildinfo`), JSON and JSX outputs, a second `--outDir` (`infm-list-*`).
- `--traceResolution` under `bundler`, `node10`, `node16`, with `--customConditions`, `--resolvePackageJsonExports/Imports false`,
  `--noResolve`, `paths`/`rootDirs`/`moduleSuffixes`, `typeRoots` and `types` (`infm-trace-*`).
- `--showConfig` for references, `composite`, JS options, `paths`, an `extends` array with `${configDir}`, many enum and list
  options, a non-ASCII `--outDir` (`infm-showconfig*`); `--init` with many options, `--pretty`, `--locale ja`, with `--listFiles` (`infm-init-*`).
- `--generateCpuProfile`/`--generateTrace` with JS, composite, `lib.es5`, a real `lib.dom.d.ts` (`infm-trace-dom`: `types.json`
  byte for byte), an unwritable profile path (`infm-cpuprofile-unwritable`); `--extendedDiagnostics` with JS, incremental reruns, `outFile`, `--noResolve`; all
  informational flags in one run (`infm-all-together`); response files `@args.txt`, nested, with `--showConfig`, a missing one (`infm-response-file`).

**Result.** All 46 cases are identical on Node v26.8.2 and on jsse `--node` (TypeScript 6.0.3). The pins do not depend on where
`TSC_DIR` or `TMPDIR` is (checked with the package at another depth under another name and `TMPDIR=/var/tmp`). `infm-trace-dom` takes about
2 s on Node and 50 to 60 s on jsse (non-LTO build, shared host); the rest run in 0.2 to 22 s on jsse, the `--build` ones slowest.

**Engine fix found by this set.** `inspector.Session#post` called its command callback bare, so an exception thrown there
unwound through `post` and killed the process. Node's `Session#onMessage` catches it and reports it with `process.emitWarning(error)`.
tsc writes the profile inside the `Profiler.stop` callback, so a `--generateCpuProfile` path that cannot be written
(`ENOTDIR`, `ENOENT`, `EACCES`) printed `(node:PID) [ENOTDIR] Error: ...` and exited 0 on Node, and crashed with a stack
trace and exit 1 on jsse. `node-lib/inspector.js` now does the same as Node; `infm-cpuprofile-unwritable` and
`tests/node_inspector_callback_throw.rs` pin it. `inspector` stays the minimal module of #1375 (no sampling profiler, #1373).

**Harness changes (added lines in `scripts/tsc_oracle_norm.py` and `scripts/run-tsc-projects-oracle.py`).**
- `@stats` handled labels without a hyphen only; `Up-to-date check time:` of `--build --extendedDiagnostics` kept the width of
  its padding, so it differed between engines. Hyphens inside a label are normalised too (counts still compared; a line that
  starts with a hyphen is not a statistic).
- `tsc -b --generateTrace` writes `trace.<pid>-<n>.json`, `types.<pid>-<n>.json` and a `legend.json` naming them: the process
  id in file names and in `legend.json` is `<PID>` (the project counter `<n>` stays). Two files that normalise to the same
  name stop the run instead of one hiding the other.
- `--explainFiles`, `--traceResolution` and `.tsbuildinfo` files print library paths relative to the directory they are written
  for (the work tree, or a project below it), which depends on where `TSC_DIR` is. `../../<path below the common ancestor>` is
  now `<TSC>` like the absolute path. Before, the pins of the cases that print such a path were valid only where they were made.
- `(node:NNN)` in a process warning on stderr is `(node:<PID>)`.
- The CPU profile shape requires `timeDeltas` to be numbers, no longer non-negative: V8 emits a slightly negative delta now
  and then (seen on Node in `infm-b-generate`), so Node itself was not stable against the check. The shape label says "deltas
  are numbers".
- **21 existing pins** (`expected-build.txt` 13, `expected-emit-options.txt` 2, `expected-projects.txt` 3,
  `expected-info.txt` 3: `build-*`, `incremental*`, `emit-composite`, `emit-incremental`, `explain-files`, `trace-resolution`,
  `info-cpuprofile*`) changed digest with these normalisers and were re-pinned from Node (`--pin <names>`); all 413 cases
  match the pins with Node standing in for jsse, with `TSC_DIR` at two different paths.

**Known differences** (`scripts/tsc-fixtures/known-differences-1247.txt`, issue per row, no case exercises them):
the profile has no samples (#1373), the top-level and `at-async` stack frames of an uncaught error (#1368, #1379), and, filed
as #1487 (under #1418), the missing `node:fs:779` source-line header and `<anonymous>:L:C` builtin frames of an uncaught error
raised inside a `node:` builtin (a case that reached an uncaught `ENOTDIR` from `--generateTrace` onto a file was dropped for
that reason). Not a jsse difference: a profile path under `/proc` (`--generateCpuProfile /proc/no/such/dir/p.cpuprofile`)
makes tsc's own directory-creation loop spin forever on Node and on jsse alike, so no case uses it.

**Acceptance of #1247.** Deterministic outputs match Node (46 more cases plus the 24 of #1375, byte for byte but for the
normalisers above); timing and memory lines match in shape (`@stats`); `tsc` is graceful with the minimal `inspector`
(a profile is written, an unwritable path is a warning as on Node); the default environment is unchanged (`node-lib/inspector.js`
is compiled into the binary but reachable only under `--node`, and the oracle edits are Python); the oracle is wired into a
runnable script and guards. The contents of a CPU profile (samples) stay a gap by nature of #1373 (no sampling
profiler) and are compared by shape.
