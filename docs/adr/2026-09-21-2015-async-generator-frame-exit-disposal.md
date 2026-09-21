# Async generators: suspendable `await using` disposal

Issue #686. Every DisposeResources site in the async-generator driver drained
the microtask queue inline (`dispose_resources` → `run_dispose_cursor_blocking`),
so a request could settle before its async disposer finished. Two further
defects came from the intact-block path ADR-2026-09-21-1007 left in place for
async generators: a block containing `yield` was replayed (its disposer ran
twice), and after #703 gave `try`/`finally` clause lists their own scope frame,
an `await using` declared there was never disposed at all.

## Decisions

- **The parked cursor is a side table, not an `IteratorState` field.**
  `Interpreter::generator_pending_dispose` (`GeneratorDisposal`: cursor + the
  in-flight request's `promise`/`resolve`/`reject`) sits beside
  `generator_for_of_stacks`/`generator_scope_stacks`, GC-rooted in
  `collect_gc_roots` and dropped in `free_gc_object`. A new
  `StateMachineAsyncGenerator` field would have meant editing every literal
  construction and splitting the shared sync/async GC pattern. The parked
  request stays at the front of the generator's queue (`async_gen_yield_pending`),
  so a later request cannot start the generator early.
- **No `EnterScope`/`ExitScope` for generators.** The two generator drivers
  route abrupt completions through `route_generator_exception` and friends,
  not `route_return!`/`route_loop_control!`. Instead the intact-block arm is
  deleted: an `await using` block lowers through the same `OpenBlock` scope
  depth as any block, and the driver disposes the frames a state transition
  leaves before `reconcile_scope_stack` drops them (`GeneratorDisposeThen::Reenter`).
  `break`/`continue` need no static exit chain, because leaving a scope is
  always a transition to a state of lower `scope_depth`.
- **`return expr;` awaits `expr` before disposing** (`sec-return-statement-runtime-semantics-evaluation`)
  when resources are pending; a generator with none keeps its existing path
  and tick counts.
- **Completing the generator disposes everything at once**: function-level
  resources plus every open frame, innermost first
  (`take_generator_dispose_stack`); a `for-of` unwind disposes frames nested in
  the loop before closing its iterator.

## Known boundary

A frame disposal reached while a throw or return is already in flight (or while
replaying an inline yield), and the `for-of` unwind above, still use the
blocking driver. A throwing disposer that replaces an in-flight `return`
re-routes as a fresh exception and does not re-enter a `finally` already
selected for that return. `for (await using x of …)` iteration-environment
disposal in the async-generator driver is also still blocking.

The lowering itself is gated on the container: an async generator lowers a
block, `try` clause, loop body, or `switch` case only when it holds a
suspension point (`stmt_has_suspension`). An `await using` in one with no
`await`/`yield` inside is still tree-walked and disposes inline, settling the
request before the microtask queue drains.
