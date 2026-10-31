# Library Tests: generator frames and loop-head positions (#1529)

Part of epic #1214 (tracker #1418). Follows `library-tests-typescript-conformance-stack-frames-1508.md`, whose
"Not fixed" section this closes. `--node` only; default mode and both engines' semantics are unchanged.

## What a generator body is in a stack

- **Live frame.** A sync or async generator body used to run without a `call_stack_frames` entry in either engine, so
  `new Error()` inside it printed the caller's frame at the generator's position. Every step now runs in a frame of the
  generator's function (`builtins/node_host_stack/generator_frames.rs`): the function is recorded in a hidden slot of the
  generator object when the call creates it (written only while the hooks are installed; the collector traces it like any
  private element), and the four drivers push a frame around each step: `bytecode_generator_resume`
  (`eval/generator_runtime/bytecode_resume.rs`), `run_step` (`eval/async_gen_chunk.rs`), and for the state machines
  `generator_next_state_machine_with` and `async_generator_next_state_machine_with_promise_and_reentry`, which every
  `next`/`return`/`throw`/await continuation reaches. The state-machine drivers also put the generator's environment on
  `call_stack_envs` for the step (`node_in_generator_frame`), as the bytecode drivers already did. The default
  parameter expressions of a generator run in the function's frame too (`at syncWithArgs`), as an async function's do.
- **`at async gen` frames.** `node_run_reaction_job` takes the request promise at the front of the generator's queue
  as the job promise of an `AsyncGen` reaction, as V8 continues from it, and `async_awaiters` prints a frame for a
  generator parked at an `Await` or `Yield` step (not `AwaitReturn`, where V8 ends the walk) and continues from the
  front request's promise. The position of the park is a second hidden slot: the bytecode driver reads the call
  position when the chunk suspends at an `await`, the state machine takes the `await`'s own offset.
- **Closure-based `await`s.** `yield*` and a disposer's `await` in an async generator suspend through `await_then`
  closures rather than a typed reaction. The continuation registered on the awaited promise is marked with the
  generator (`node_mark_generator_await`), so the walk recognises it as one of its `Await` steps.

## Positions of loop heads

V8 reports the head of a loop for every step of its iterator (`for (const s of gen())` is the column of `s`, also in a
frame below the generator) and for the `await`s of a `for await`. `ForOfStatement.at` carries it (the source offset plus
one, like every position the parser records, `parser/positions/for_head_site.rs`): the start of the binding, the closing
bracket of a declared pattern, the start of an assigned pattern, a member's property (or its `[`). It reaches:

- the bytecode compiler, as a `SetCallPos` before each `IterStep` (`compiler/for_of.rs`; no bytes unless the hooks are on);
- the tree-walker's `exec_for_of_loop`;
- the `ForOfHead` terminator of the three state-machine drivers (async function, sync generator, async generator),
  which set the call position before stepping the iterator and, for `for await`, note the `await` position.

A function parked at a disposer's `await` (`await using` at the end of a scope) reports where V8 last recorded a
position: the latest call, else its last `await` (`node_note_dispose_pos`). V8 also counts statements (`return`, `throw`,
a `const x = 1 + 2` declaration), which the engines do not record.

One latent bug is fixed on the way: `node_push_async_module_frame` zeroed the `await` position of every async state
machine on resumption, not only a module's.

## Verification

`scripts/v8-error-fixtures/generators-1529.js`, with Node 26.8's output pinned in `expected-generators-1529.txt`
(regenerate with `node scripts/v8-error-fixtures/generators-1529.js > .../expected-generators-1529.txt`), covers sync
generators (functions, methods, static methods, object literals, `yield*` of a call, a finished `yield*`, a throwing one,
`return()` and `throw()` into a `finally`, default parameters, `next()` called directly, spread, `Array.from`), async
generators before and after an `await` or a `yield`, awaiting an async function or `Promise.all`, a thrown error, two
queued requests, every kind of `for await` head, and `await using` in async functions. `tests/node_stack_frames_1529.rs`
replays it under the VM, the tree-walker and `JSSE_GC_STRESS=7`, checks the `CallSite` API of a generator frame and
checks default mode. No `test262-extra/` test: nothing here is observable through ECMAScript, only through the host's
`Error.stack`.

A call through a property named like a reserved word (`it.return(7)`, `p.catch(f)`) is reported by V8 at its
parenthesis, not at the name; `call_site` (`parser/positions.rs`) did the latter. Found while pinning the fixture.

## Not matched (tracked under #1418)

`scripts/v8-error-fixtures/generators-1529-gaps.js` runs each of these on Node and on `jsse --node`; nothing replays it.
The frames exist in all of them, only the position differs:

- `yield*` in an async generator parked on the delegation (`at async gen`): V8 reports the `yield*` keyword; the AST
  records no offset for `yield`, so the frame is at the function start. A sync `yield*` of a call reports the call, as
  V8 does; `yield* iterator` of anything else (V8: the `yield*` keyword) is at the function start.
- An `await using` in an async generator whose scope ends: V8 reports the last statement or expression position (the
  `yield 1` before it), and the caller of a `break`ing `for await` the `break`. The engines record call and `await`
  positions only, so `await using` in an async *function* matches only when a call or an `await` is the last thing
  before the scope ends (V8 also counts `return`, `throw` and declarations).
