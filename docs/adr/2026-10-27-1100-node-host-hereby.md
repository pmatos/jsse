# Node host mode: hereby and its dependencies

Status: accepted, epic #1214, issue #1251 (extends `2026-10-26-0854-node-host-mode.md`).

## Context

The TypeScript repo builds and tests itself with `hereby` (`Herebyfile.mjs`, an ES module). `hereby 1.12.0`
loads `picocolors`, `pretty-ms`, `fastest-levenshtein` and the Herebyfile's own imports: `chokidar 4`,
`glob 10` (with `minimatch`, `path-scurry`, `foreground-child`, `cross-spawn`, `which`), `@esfx/canceltoken`,
`minimist`, `jsonc-parser`, `esbuild`. Before this change `hereby --tasks` died with a SyntaxError.
The TypeScript tree is the one pinned by #1250 (v6.0.3, `npm ci`); nothing is vendored.

## What was missing

Walking the import graph of hereby and of every `devDependency` found four gaps, all fixed here:

1. **Parser: class fields named `get` or `set`** (#1313). `class A { set; }` was read as an accessor
   without a name, so `minimatch` (and therefore `glob`) did not parse. After `get`/`set`, a `=`, `;`, `}` or
   `*` now makes it a field (the `*` case is the existing ASI rule); a name or `(` still starts an accessor
   or method. The two near-identical branches were merged and the field tail (initializer, `arguments` early error,
   terminator) is one helper, `finish_class_field`, so `declarations.rs` shrinks. Test:
   `test262-extra/class-fields-named-get-and-set.js`. The vendored `node-lib/internal/deps/minimatch`
   copy is restored to upstream's `set;` (the quoted workaround recorded in the node-host ADR list of vendored
   edits is gone; that list is not edited here).
2. **`global`.** Node's alias of the global object (`writable`, `enumerable`, `configurable`) was missing, so
   `isexe` (a dependency of `which`, hence `cross-spawn`, hence `foreground-child`, hence `glob`) threw
   `ReferenceError: global is not defined`. It is defined by `scripts/node-process-shim.js` (the `--node`
   prelude only; the default environment is unchanged).
3. **`node:constants`.** `foreground-child` imports it for the `SIG*` names. `node-lib/constants.js` is
   `Object.assign({}, os dlopen, errno, priority, signals, fs)` frozen, in Node's order; Node appends the
   crypto constants (not zlib's). jsse's `crypto` has no `constants` yet and its `fs.constants` has 54 keys
   to Node 26's 60, so `Object.keys(require("constants")).length` differs from Node until those land.
4. **ES module frames.** Frames of functions defined in an ES module were `<anonymous>` or a bare path.
   They now read `file:///abs/path.mjs:line:col` like Node: when a frame is recorded for a function whose
   closure sits in a module environment (`Environment::find_module_path`), its source is named by the
   module's `file:` URL (`NodeStackState::name_module_source`, by source identity, so identical files do not
   share a name; nothing is read or copied). Percent-encoding follows the URL path set.

Not needed and not done: `chokidar` (uses `fs.watch`, #1235), `glob`, `picocolors` (`isColorSupported`,
`FORCE_COLOR`, `NO_COLOR`, `TERM=dumb`), `@esfx/canceltoken`, `which`, `minimist`, `pretty-ms` already behave
as on Node; the oracle pins that.

## Known gaps (filed)

- The module top-level frame and `at async` frames of stacks (#1368) are no longer normalised away: the oracle
  drops only `node:internal` frames, and every user frame must match.
- `null`/`true`/`false` as module export names (#1367): `zod/mini` (via `knip`) does not parse. Lint only.
- `eslint`, `typescript-eslint`, `playwright`, `monocart-coverage-reports`, `azure-devops-node-api` need
  `worker_threads`, `http`, `net` (lint and coverage tickets, not the build chain).
- A task that spawned `process.execPath` started a bare jsse without `--node`
  (`hereby build-src`'s first step, `processDiagnosticMessages.mjs`, failed that way).
  Fixed in #1253 (`2026-10-27-1600-node-host-hereby-local.md`).

## Verification

`TS_SRC_DIR=<TypeScript checkout> uv run python scripts/run-hereby-oracle.py` runs 32 cases on Node and on
`jsse --node` and diffs stdout, stderr and the exit code: the TypeScript Herebyfile (`--tasks`,
`--tasks-simple`, `--help`, `--version`, a missing task, a typo with "Did you mean", a missing Herebyfile,
colour, 50 and 140 column pseudo-terminals), the fixtures in `scripts/hereby-fixtures` (a task graph, failing
sync/async/string tasks, two failures at once, a self-referencing task, a duplicate name, no exports, a throw
while loading, no Herebyfile, colour environments, a pseudo-terminal) and `deps/deps.mjs` (glob, picocolors,
canceltoken, which, minimist and a live chokidar watcher, waiting on its events, never on a timer). It
normalises paths, durations, pids and the frames listed above. It never runs a build task (`hereby local` is #1253).
Without `TS_SRC_DIR` (or with another TypeScript version) it prints `SKIP:` and exits 0; `--require` makes
that an error. Guards that always run: `tests/node_hereby_deps.rs` (`global`, `constants`, ES module frame
names; it also drives the oracle when `TS_SRC_DIR` is set) and `scripts/test_run_hereby_oracle.py` (CI
`unittest discover`). Result on Node v26.8.2: 32/32 identical. The default environment and test262 are untouched.
