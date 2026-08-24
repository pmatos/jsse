# Pending promise resolver rooting

## Problem

JSSE currently places resolver functions retained by host-async work on
`gc_temp_roots`. That stack is owned by evaluator frames and is intentionally
truncated back to an entry marker on every normal and abrupt exit. A resolver
created by `$262.agent.getReportAsync` or `Atomics.waitAsync` therefore loses
its only traced reference as soon as any enclosing call frame returns. If a
collection runs before the host completion, the resolver is reclaimed and the
still-reachable promise remains pending forever.

ECMA-262 models deferred work with a PromiseCapability Record containing the
promise and its resolve and reject functions. `CreateResolvingFunctions`
creates both functions over the promise and shared already-resolved state, and
the `Atomics.waitAsync` Waiter Record retains the whole capability until its
resolve job runs. JSSE's Rust completion closures retain raw `JsValue`s that
the tracing collector cannot inspect, so the equivalent object edges must be
represented explicitly.

## Design

Store the resolve and reject functions created for an intrinsic promise in
that promise's `PromiseData`. While the promise is pending, the Promise object
tracer marks both functions in addition to the existing result and reaction
edges. Settlement clears these reverse edges because no future host completion
needs them to keep the already-settled promise usable.

With resolver lifetime derived from the Promise object graph, remove the
`gc_temp_roots` pushes and corresponding delayed unroots in
`$262.agent.getReportAsync` and `Atomics.waitAsync`. Evaluator frame cleanup
then returns to a single invariant: every entry on `gc_temp_roots` is
frame-scoped and may be truncated to a saved marker. The shared object tracer
is used by both the tree-walking evaluator and bytecode VM, so the ownership
rule is tier-independent.

## Alternatives considered

1. Keep persistent async roots in a second interpreter registry. This fixes
   frame truncation but preserves manual add/remove bookkeeping at every host
   completion and requires auditing future async builtins.
2. Add traced root lists to each scheduler completion. This models the host
   closure precisely once it has been queued, but the resolver must also live
   while a background thread is waiting to enqueue that completion.
3. Replace every bulk frame truncate with value-by-value cleanup. This cannot
   establish which roots outlive a frame, complicates abrupt paths, and leaves
   the incompatible lifetimes mixed together.

## Validation

Add `test262-extra` async regressions for both host-async producers. One creates
a pending `$262.agent.getReportAsync` promise, returns from that call, forces
`$262.gc()`, then starts an agent that publishes the awaited report. The other
starts a finite `Atomics.waitAsync`, forces collection, and requires its promise
to resolve to *"timed-out"*. Both tests must fail to settle before the fix and
settle after it. Run them through both the normal evaluator and the
bytecode-enabled CLI path, then run the upstream `Atomics.waitAsync` directory,
custom tests, and the repository's full quality gate and test262 suite.
