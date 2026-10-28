# Node host mode: `at async` frames

Status: accepted (#1368), epic #1214. Extends
`2026-10-26-1000-node-host-v8-errors.md`; the module top-level frame itself
came with `2026-10-27-0540-node-host-fatal-report.md`.

## Context

V8 prints the functions that are suspended in an `await` on the chain of
promises that leads to the running one, after the live frames:

```
Error: y
    at f (file:///b.mjs:4:36)
    at async g (file:///b.mjs:5:21)
    at async file:///b.mjs:6:1        <- module top-level await
```

jsse printed only `at f`. Tools that show `error.stack` of a failed task (hereby
for a Herebyfile that throws while loading, mocha, the TypeScript test runner)
therefore printed a shorter trace than Node, and their oracles normalised the
`at async` lines away.

## Decision

1. **V8's walk.** A stack captured while a promise reaction job runs continues
   from the promise that job settles: the result promise of the async function
   an `await` resumes, else the reaction's derived promise. While the promise is
   pending and has exactly one reaction, an `await` reaction (`AsyncResume`) of
   a parked async function adds a frame for it and continues from its result
   promise; any other reaction continues from its derived promise, without a
   frame. Anything else (a settled promise, two reactions, a combinator, an
   async generator) ends the walk. The walk only reads promise records and the
   parked state of async functions: no getter, `then` or script runs. It is in
   `builtins/node_host_stack/async_frames.rs` and shares the stack limit with
   the live frames.
2. **The current job.** `Interpreter::node_run_reaction_job` notes, only once the
   hooks are installed, the promise the reaction job settles in
   `NodeStackState::job_promise` (saved and restored around the job). Frames made
   by synchronous calls nested in the job (an async function called from a
   `then` handler, `new Error()` in a helper) get the same async tail, as in V8,
   where the isolate's current microtask decides. A dynamic `import()` that
   evaluates its module in a deferred job runs it as the settlement of the
   promise it returned (`node_with_job_promise`), so a throwing Herebyfile
   continues with `at async loadHerebyfile`, `at async mainWorker`, ...
3. **Where the `await` is.** The frame's position is the `await` keyword, as
   in V8. `Expression::Await` carries its offset plus one like `Statement::Throw`
   does. A bytecode async function emits `Op::SetCallPos` before the suspending
   `Op::Yield` (only while the hooks are on, so default chunks are unchanged);
   `run_async_activation` reads `call_pos` before the frame pops and keeps it in
   `AsyncActivation::await_pos`. The state machine behind module top-level
   `await` (and `for await`, `await using`, which have no position) keeps it in
   `AsyncFunctionState::await_pos`, set by the `StateTerminator::Await { at }`
   arm and cleared on each resumption. Async functions are lowered by rewriting
   `await` to `yield` and back; `generator_transform/await_pos.rs` carries the
   offset through the rewrite as `(offset, operand)` in a `Comma`, only while the
   hooks are on; a `yield` the transform hoists into a temporary keeps it.
4. **The frame.** The function is the activation's (`func_obj_id`; the state
   machine records it too), the receiver its `this` or an arrow's call receiver,
   so a method reads `at async Worker.start`. A module body is a nameless
   top-level frame, `at async file:///b.mjs:6:1`. The record has a new flag bit
   (4); the prelude's `CallSite.isAsync()` is true for it and `toString()`
   prefixes `async `.

## Verification

`scripts/event-loop-fixtures/async-stack.fixture.mjs` (cross-checked against Node
by `run-node-event-loop-oracle.sh`, its output pinned in `async-stack.stdout` and
replayed by `tests/node_v8_error.rs` under the VM and `JSSE_GC_STRESS=7` and
`1`) covers the issue's repro, a `then` handler awaited by an async function,
methods and a static method, a handler that calls an async function, a throw
before the first `await`, `Error.stackTraceLimit` cuts, and a stack made after a
top-level `await`. `scripts/v8-error-fixtures/cases.js` adds a CommonJS chain.
`scripts/run-hereby-oracle.py` no longer normalises module top-level or
`at async` frames away: it drops only `node:internal` frames (hereby's failing
Herebyfile load now matches Node through `at async run`).

## Known gaps (versus Node 26)

- Node's own `at ModuleJob.run` / `at async node:internal/...` frames, which the
  oracles drop (jsse's embedded modules have other line numbers).
- `at async Promise.all (index N)` (and `any`/`allSettled`), and async
  generators: their reactions are not followed.
- Under `--no-bytecode` a state-machine async function has no live frame of its
  own, but it does appear as an `at async` frame when awaited.
- A `for await` or `await using` suspension has no position and reports the
  function start.
