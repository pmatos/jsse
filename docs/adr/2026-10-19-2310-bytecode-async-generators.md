# Async generator bodies as suspending bytecode chunks

Status: accepted (#1105, second stage); extends ADR-2026-10-19-2030 (async
functions) and ADR-2026-10-17-0640 (sync generators).

## Context

An async generator body ran on the state machine: tree-walked payloads, the
per-resume scope and for-of reconciliation, and a request queue driven through
`async_generator_next_state_machine_impl` (1,800 lines). `async-fs` spent
most of its 7.7 G Ir there: its untimed setup fill loop (`randomFileContents`,
5.6 G) and the `ls` generators are async generator bodies.

## Decision

1. **Same chunk, two suspension kinds.** The body is one chunk; `yield` and
   `await` both suspend it as `Op::Yield` (a `yield` has a return stub and a
   positive skip offset, an `await` neither; `GeneratorFrame::suspended_at_await`
   tells them apart). `compile_async_generator_body` is the async function
   body compiler with `yield` as well; `yield*`, `for await`, `using` and
   everything either kind already refused keep the state machine.
2. **The compiler puts the spec's awaits in the chunk.** `yield e` is
   `e; await; yield` (AsyncGeneratorYield awaits the operand); the `return(v)`
   stub after the yield is `await; <return exit>` (AsyncGeneratorUnwrapYieldResumption
   awaits `v` once, then the completion unwinds through `finally` blocks and
   iterator closes like a `return`); `return e;` is `e; await; <return exit>`
   and a bare `return` awaits nothing. A rejection raised by any of these
   awaits is thrown at that op, inside the enclosing `try`.
3. **The queue is the scheduler's.** `IteratorState::BytecodeGenerator`
   carries an `is_async` flag (no new variant: a second variant measurably
   slowed unrelated GC-heavy workloads by ~1% through code placement).
   `async_gen_enqueue` and the prototype methods are unchanged; the
   front request stays at the head of `async_gen_queues` while the body runs
   or awaits, so a request made meanwhile only waits.
   `async_gen_process_queue` hands a chunk generator to `async_gen_drain`
   (`eval/async_gen_chunk.rs`), which serves the front request by state
   (suspendedStart / suspendedYield / completed), settles it with
   AsyncGeneratorCompleteStep (pop first, then resolve), and repeats until the
   queue is empty or the body parks at an `await`; that `await`'s reaction
   (`ReactionTarget::AsyncGen(id, Await)`, dispatched in `async_gen_reaction`)
   re-enters the loop. A completed generator answers `next` and `throw`
   inline and `return` through the existing AsyncGeneratorAwaitReturn.
4. **Roots and tail calls** as for async functions: the frame is stored on the
   generator before PromiseResolve runs user code, the generator is traced by
   the existing bytecode-generator arm, and a run suppresses tail calls.

## Consequences

- Tick counts of requests, yields, returns and awaits are the spec's by
  construction and match the state machine's (checked against node in the
  test262-extra files).
- Spec difference from the state machine: a request served while the generator
  is executing never throws "already executing"; it queues (AsyncGeneratorEnqueue).
- `yield*` in an async generator (a delegating op with three resumption kinds)
  and `for await` remain on the state machine; they are the next stage.
