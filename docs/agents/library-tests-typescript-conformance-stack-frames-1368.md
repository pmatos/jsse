# Library Tests: stack-frame group (#1333, #1358, #1368, #1379)

Part of epic #1214. The `--node` host prints V8-shaped `error.stack` text
(`docs/adr/2026-10-26-1000-node-host-v8-errors.md`, `2026-10-28-1400-node-host-async-frames.md`).
This file records what the four stack-frame issues needed once the bytecode VM had most of it, and what is left.

## State per issue

- **#1368** (module top-level frame, `at async` frames): met in the bytecode VM by earlier work
  (`scripts/event-loop-fixtures/async-stack.fixture.mjs`). The tree-walker now matches too (below).
- **#1333** (`at async` after an `await`, `captureStackTrace(o, c)` inside an async `c`): the `at async`
  walk existed, but `captureStackTrace(o, fn)` with `fn` at the bottom of the live stack was read as
  "`fn` is not on the stack" and captured nothing. `async_frames.rs` now asks whether `fn` is on the live
  stack. If it is not, V8's `SKIP_UNTIL_SEEN` also applies to the async frames: they are dropped up to and
  including the async function `fn` (`f(e)` in `async.js`). The `assert` oracle no longer patches the
  `err.stack` match of `test-assert-async` out (`scripts/assert-fixtures/manifest.txt`).
- **#1358** (tree-walker omits the async function frame): a state-machine async function
  (`--no-bytecode`, and any async body the bytecode compiler declines) pushes a call frame for itself on
  every resumption (`node_push_async_frame`), as the bytecode VM's async activations do. An async arrow keeps
  its call receiver in a hidden `%receiver` binding of its environment, so the collector traces it and
  `Context.late` / `Object.early` read as in V8, also while parked. `tests/node_fatal_report.rs` no longer
  skips `fatal-reject-async` on the tree-walker, and `tests/node_v8_error.rs` no longer cuts the
  `async before await` / `async arrow body` sections from the tree-walker replay.
- **#1379** (arrow receivers, inferred names, key order): already met in both engines
  (`scripts/v8-error-fixtures/names.js`); the tree-walker replay was hiding only the async arrow case, which
  is fixed above.
- Default parameter expressions of an async function run in the function's frame (`at withDefault`), in both
  engines; they ran before any frame existed.

## Verification

`scripts/v8-error-fixtures/async.js` (Node 26.8's output pinned in `expected-async.txt`, added to the
`NAME` list of `scripts/run-node-v8-error-oracle.sh`) covers methods, static methods, nested async functions,
a throw before the first `await`, loops and `finally`, a then handler, default parameters, async arrows with a
receiver, the #1333 and #1358 repros, and `captureStackTrace` skipping each frame of the chain, a stranger and
nothing. `tests/node_stack_frames_1368.rs` replays it under the VM, the tree-walker and `JSSE_GC_STRESS=7`,
replays `async-stack.fixture.mjs` on the tree-walker and checks default mode stays empty in both engines.
`TS_SRC_DIR=... scripts/run-hereby-stacks-oracle.py`: 15 cases as expected; none of the five known failures
(`ts-suite/hereby-stacks-known-failures.json`: #1482, #1483, #1484) agrees yet, so the ratchet is unchanged.

## Remaining differences (tracked under #1418)

- `at async Promise.all (index N)` (also `any`, `allSettled`): the walk ends at a combinator.
- A frame called through a property name that differs from its function name lacks V8's ` [as method]`
  suffix (`Timeout.tm [as _onTimeout]`); the timers shim leaks its own `timers:` frames.
- Async generators have no `at async` frames, and a `for await` or `await using` suspension has no
  position.
