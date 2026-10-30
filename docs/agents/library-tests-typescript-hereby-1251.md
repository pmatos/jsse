# Library Tests: hereby stacks oracle (#1251)

Companion to the **hereby oracle** entry of [library-tests-typescript.md](library-tests-typescript.md), which
covers `hereby --tasks`, `--help`, task graphs, failing tasks, colour, ptys and chokidar/glob/picocolors/
`@esfx/canceltoken` (32 cases, all identical to Node; design `docs/adr/2026-10-27-1100-node-host-hereby.md`).
This file covers the criterion "task graph errors report with correct stacks".

`TS_SRC_DIR=<TypeScript v6.0.3 checkout, npm ci done> uv run python scripts/run-hereby-stacks-oracle.py [--jobs N] [--jsse BIN] [--require] [case ...]`
runs the hereby CLI on the `stack-*` fixtures of `scripts/hereby-fixtures` under Node and `jsse --node` and diffs
stdout, stderr and the exit code with the normalisation of `scripts/run-hereby-oracle.py` (loaded as a module:
paths, durations, pids, and only `node:internal` frames dropped, so every user frame, module top-level frame and
`at async` frame must match). hereby prints `e.stack` only for an error thrown while a Herebyfile loads (a task
that fails prints its message), so the fixtures fail at load:

- `stack-import-throws`, `stack-class`: thrown from an imported module, through a class (static method,
  constructor, instance methods, `for...of`).
- `stack-tla`: after top-level `await` and a timer (`at async` chain).
- `stack-cjs`: through `createRequire` into a CommonJS module (the `Module._compile` chain is `node:internal`).
- `stack-cause`, `stack-bad-task` (hereby's own `TypeError`), `stack-not-error` (a thrown plain object),
  `stack-pty`, `stack-colour`.

Every case runs through a launcher that sets `Error.stackTraceLimit = 200` and imports hereby, so the different
number of `node:internal` frames cannot push user frames over the default limit of 10 on one side only.

`ts-suite/hereby-stacks-known-failures.json` maps the cases that still differ to the issue that tracks them. A
listed case that now agrees, a case that differs and is not listed, or a name that matches no case exits 1:

| case | difference | issue |
| --- | --- | --- |
| `stack-native` | V8 prints `at Array.map (<anonymous>)` for a builtin that called back; jsse has no native frames | #1482 |
| `stack-missing-import`, `stack-missing-package` | the header now matches (`Error [ERR_MODULE_NOT_FOUND]`); an error created inside `import()` shows neither the caller's call position nor the calling async function's frame | #1483 |
| `stack-syntax`, `stack-missing-export` | parser wording (`Unexpected token '='`) and the link error text and decorated source line | #1484 |

Fixed here: errors made by `node-lib/internal/errors.js` (`E(...)`) now carry Node's `kIsNodeError` marker (an own
non-enumerable symbol, as in Node) and `scripts/node-v8-error.js` prints their stack header as
`Name [CODE]: message` like Node's `prepareStackTrace`. Errors built by the other host shims
(`node-fs-support.js`, `node-process-shim.js`, `node-event-loop-shim.js` have their own `nodeError`) keep the plain header.

Guards: `tests/node_hereby_stacks.rs` (header, plain errors, a missing module, and the oracle when `TS_SRC_DIR` is
set), `scripts/test_run_hereby_stacks_oracle.py` (case table, ratchet, skip behaviour). Result: 14 cases, 9 identical
to Node v26.8.2 and 5 known failures; the 32 cases of the base hereby oracle still pass.
