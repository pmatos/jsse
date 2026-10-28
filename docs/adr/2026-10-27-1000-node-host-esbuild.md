# Node host mode: the esbuild JS API and its native service

Status: accepted, epic #1214, issue #1252 (builds on #1236 child_process, #1233 stream, #1241 esm interop).

## Decision

1. **No engine or Node-surface change was needed.** esbuild 0.27.3 (the version `package-lock.json` of
   TypeScript v6.0.3 resolves) runs unmodified under `jsse --node` with the modules the other children of #1214
   added. `lib/main.js` spawns `@esbuild/linux-x64/bin/esbuild --service=0.27.3 --ping` with
   `stdio: ['pipe', 'pipe', 'inherit']`, calls `child.unref()`, `stdin.unref()` and `stdout.unref()`, and
   re-`ref`s the child through a counter while a request, a plugin callback or a live context needs it. That
   is exactly the shape `child_process` (ADR `2026-10-26-2300-node-host-child-process.md`) already supports:
   a socketpair per pipe, loop liveness decided in JS, EOF on the child's stdin when the parent exits. The
   deliverable is the harness that keeps it that way.
2. **esbuild is not vendored.** `ESBUILD_DIR` names an installed `esbuild` package with its platform binary in
   the sibling `@esbuild/<platform>` directory (or `ESBUILD_BINARY_PATH`); `npm ci --ignore-scripts` in the
   TypeScript checkout of `ts-suite/pin.json` (#1250) provides both. The oracle and `tests/node_esbuild.rs` skip
   with a notice for anything else, so CI, which has neither, never runs the real-esbuild parts.
3. **Oracle.** `scripts/run-node-esbuild-oracle.sh` runs each `scripts/esbuild-fixtures/*.fixture.js` in a fresh
   copy of `scripts/esbuild-fixtures/project` on Node and on jsse and diffs stdout and the exit status (and
   stderr, with `Done in Nms` and the scratch path scrubbed, for the one fixture that logs through the native
   binary). The service binary is run through a wrapper that records its pid; after each fixture every recorded
   service must be gone, so a leaked child fails the run, and a hang fails on the per-fixture timeout.
   Fixtures: `build-cjs` (the Herebyfile's `createBundler` options: node, cjs, linked sourcemap, banner, footer,
   define, external, a post-processing `onEnd` plugin, metafile), `build-esm-split` (splitting, hashed names,
   text/file/dataurl loaders, minify, external sourcemaps, iife and inline maps), `transform` (transform,
   `transformSync`, `buildSync`, `formatMessages`, `analyzeMetafile`), `plugins` (every callback, `build.resolve`,
   plugin data, thrown and returned errors, virtual modules, the TypeScript build's `onLoad` remap),
   `context` (rebuild, parallel rebuilds, watch with `onEnd`, cancel, dispose twice, `stop`), `errors`,
   `large` (a 4 MiB message and 40 concurrent builds on the one service), and four `lifecycle-*` fixtures:
   no `stop()` yet the process exits by itself, `process.exit(3)` with a build in flight, the service killed under
   a live context ("The service is no longer running"), an unhandled failed build (exit 1).
   `typescript-bundle` bundles the real `src/typescript/typescript.ts` through `TS_SRC_DIR` with the
   `exportIsTsObject` options: `typescript.js` (9,143,356 bytes), its map and the metafile are identical to
   Node's.
4. **Pinned digests.** `--pin` writes the sha256 of Node's stdout per fixture to
   `scripts/esbuild-fixtures/expected-sha256.txt`; the fixtures print no absolute path (the scratch directory and
   `ESBUILD_DIR` are scrubbed) and no length that depends on one, so the digests hold on any checkout directory
   and any esbuild install location. `tests/node_esbuild.rs` compares jsse against them
   without Node, and re-runs five fixtures under `JSSE_GC_STRESS=7`.
5. **CI-visible guard.** Because CI cannot install esbuild, two tests need nothing: a stand-in service (jsse
   itself, `stdio: ['pipe', 'pipe', 'inherit']`, the same unref/ref counter) echoes a 6 MiB message through the
   pipes, the client's loop stays alive until the echo is complete and ends with the child still running, and the
   child sees EOF and exits (its marker file is awaited, not slept for); the same under `JSSE_GC_STRESS=23` with
   160 KiB.
6. **The sync API has no worker.** `worker_threads` does not exist, so `buildSync` and `transformSync` take
   esbuild's own fallback (`execFileSync` of a one-shot service fed on stdin) instead of Node's
   `Worker` + `Atomics.wait`. Results are identical; plugins are rejected in the sync API in both.
7. **`defaultWD` is captured when `esbuild` is first required** (`process.cwd()` at that moment), on Node and jsse
   alike; a script that `chdir`s afterwards must pass `absWorkingDir`.

## Result

Node v26.8.2 against `jsse --node` (quick release build), esbuild 0.27.3: all twelve fixtures identical,
including the TypeScript bundle (about 0.45 s on Node, 0.9 s on jsse). The bundle under `JSSE_GC_STRESS=3`
(889 s) produced the same three files byte for byte. No service child is left behind. No engine change: the
default environment and test262 are untouched.
