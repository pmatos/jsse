# Resolved local binding write design

## Goal

Reduce repeated environment-map work when executing assignments in broad,
one-shot call graphs without changing ECMAScript reference, binding, or global
object semantics.

The motivating Mandreel profile spends about 18% of cold execution in
identifier resolution. For an assignment to an ordinary function-local
binding, JSSE's assignment shortcut first confirms the local binding and then
`env_set` probes the same binding once to choose an action and again to write
the value.

## Specification constraints

`EvaluateCall`, `PrepareForOrdinaryCall`, and
`FunctionDeclarationInstantiation` require a logically fresh activation and
the prescribed parameter, `arguments`, var, lexical, and closure semantics.
Assignment evaluation captures a Reference Record before evaluating the
right-hand side; `PutValue` later writes through that captured record.

The optimization may therefore combine the mutability check and value update
when `env_set` reaches a declarative Environment Record. It must preserve
temporal dead zone, constant, immutable, function-name, global-object, import,
`with`, and deleted binding behavior.

## Design

Deepen `env_set` so each Environment Record is probed once with a mutable map
lookup. If a binding is found in a record that has no global object:

- mutable bindings are written through the live map entry;
- uninitialized lexical bindings report the existing TDZ error;
- initialized `const` bindings report the existing assignment error;
- function-name and immutable bindings retain their strict/sloppy behavior.

If the record has a global object, retain the existing deferred action: release
the environment borrow, perform the potentially re-entrant global object or
proxy write, then update the binding mirror. Indirect imports, absent bindings,
and parent traversal retain their existing paths.

This is a change inside the existing storage seam. It does not expand the
already-large assignment dispatcher, omit an Environment Record, change
reference capture timing, hold a mutable borrow across user code, or specialize
based on function hotness.

## Alternatives considered

Slot-indexed function frames could remove most string hashing and scope-chain
walking, including for straight-line code, but require parser/body metadata,
new binding storage, eval and closure deoptimization rules, and a wider
correctness surface.

Writing directly inside `eval_assign` was prototyped first. Although it avoided
`env_set`, it expanded the large dispatcher and regressed cold Mandreel CPU
time from about 28-29 seconds to 34-36 seconds, so it was discarded.

Reusing a cached environment location at each AST identifier site would help
loops after warmup, but not the motivating one-shot sites and would require
invalidation for `eval` and `with`.

The single-probe `env_set` write is the smallest measured vertical slice: it
removes redundant work for translated-C register assignments while keeping the
same out-of-line control-flow shape and observable slow paths.

## Verification

- Add focused coverage for mutable local writes, constant failures, sloppy and
  strict named-function bindings, closure writes, global binding mirrors, and
  `with`.
- Compare cold Mandreel setup wall time and a `perf` profile before and after.
- Run the language assignment, function-code, and environment-sensitive
  test262 areas, followed by the full suite against the `origin/main`
  baseline.
- Run formatting, Clippy, the release build, and release tests.

## Measured result

On CPU-pinned adjacent A/B runs, cold Mandreel initialization used 14.7% to
20.9% less user CPU time than the exact `origin/main` binary (15.4% median
paired reduction across three pairs). A focused 20-million-iteration local
assignment check improved from 8.80 to 8.34 seconds and from 8.64 to 8.01
seconds in reversed run order.

In `perf` profiles of the cold Mandreel load, `env_set` self-samples fell from
2.94% to 1.17%. The broader identifier-resolution cost remains visible and is a
candidate for later, separately scoped work.
