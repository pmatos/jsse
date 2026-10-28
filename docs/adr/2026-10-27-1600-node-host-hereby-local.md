# Node host mode: `hereby local` builds the compiler on jsse

Status: accepted, epic #1214, issue #1253 (extends `2026-10-27-1100-node-host-hereby.md`).

## Context

`hereby local` is the TypeScript repo's own build (`Herebyfile.mjs`, v6.0.3 pinned by #1250): it
generates `diagnosticInformationMap.generated.ts`, type-checks and emits `src/` with `tsc -b` using
`node_modules/typescript` (5.9.3), bundles `tsc.js`, `_tsc.js`, `typescript.js`, `tsserver.js`,
`typingsInstaller.js` and `watchGuard.js` with esbuild (a native service child over stdio, #1252),
copies the libs, writes the localised diagnostics and the `tsserverlibrary` wrappers, and bundles the
declaration files. Every step is a child process: the Herebyfile runs `process.execPath` for the
generator scripts and for `tsc -b`.

## Decision

1. **`process.execPath` children are Node programs.** `hereby` spawns `process.execPath` and expects a
   Node. jsse runs Node programs only under `--node` (`fork` already added the flag). The argument
   normaliser behind `spawn`, `spawnSync`, `exec*` and `execFile*`
   (`node-lib/internal/child_process/normalize.js`) now prepends `--node` when `file` is exactly
   `process.execPath`, no shell is used and the leading flags do not already include `--node`. This changes nothing
   without `--node` (the module is part of the prelude). Observable difference from Node:
   `child.spawnargs` and error `spawnargs` show the extra argument; a shell string that names the binary is not
   rewritten. `tests/node_hereby_local.rs` pins the rules and `run-node-child-process-oracle.sh` stays green.
2. **One oracle script**, `scripts/run-hereby-local-oracle.py` (needs `TS_SRC_DIR`, else `SKIP`). It copies
   the checkout twice (`cp --reflink`), drops the ignored build output, runs `hereby local` under Node and under
   `jsse --node`, then diffs
   - the build logs as sorted lines (tasks run in parallel) with durations, paths (including `~`) and the
     interpreter path normalised, and
   - every file under `built/` plus the two generated sources, by path, size and sha256 after replacing the
     copy's own path (maps and bundles are otherwise byte-identical, there are no timestamps in the output),
   and finally packages the jsse-built `built/local` as `lib/` of a `typescript` package and runs the tsc
   project oracle (`run-tsc-projects-oracle.py`: the base, build, diagnostics, informational-flag and emit-matrix
   cases) on it, on both engines. A timeout kills the whole process group.
3. **Known gap, closed by #1378.** `dts-services` formats the declaration bundles with dprint's WebAssembly
   plugin and jsse had no `WebAssembly`, so `hereby local` ended with
   `Error in dts-services ... WebAssembly is not defined` and the oracle accepted exactly that failure, leaving
   the four d.ts bundles (`typescript.d.ts`, `typescript.internal.d.ts`, `tsserverlibrary.d.ts`,
   `tsserverlibrary.internal.d.ts`) out of the comparison. `--node` now has `WebAssembly`
   (`2026-10-28-1200-node-host-webassembly.md`); the oracle tolerates no failing task and compares every file
   (`--full` is accepted and does nothing).

## Cost

On a 24-core host at load average about 67 (shared), Node builds in 20 s and jsse in about 23 minutes, almost
all of it `tsc -b src/...` type-checking the TypeScript sources on the interpreter (peak RSS about 2.5 GB for that
process). Run it with `--timeout` suitable for the host; it is not part of the default test run
(`tests/node_hereby_local.rs` drives it only with `JSSE_HEREBY_LOCAL_ORACLE=1` and `TS_SRC_DIR`).

## Verification

`TS_SRC_DIR=<checkout> uv run python scripts/run-hereby-local-oracle.py` (see `docs/agents/library-tests.md`).
Guards that always run: `tests/node_hereby_local.rs` and `scripts/test_run_hereby_local_oracle.py` (CI
`unittest discover`). The default environment and test262 are untouched.
