# Resolved local binding write design

## Goal

Reduce repeated environment-map work when executing assignments in broad,
one-shot call graphs without changing ECMAScript reference, binding, or global
object semantics.

The motivating Mandreel profile spends about 18% of cold execution in
identifier resolution. For an assignment to an ordinary function-local
binding, JSSE currently resolves the binding before evaluating the right-hand
side and then rediscovers the same binding several times while checking
mutability and writing the value.

## Specification constraints

`EvaluateCall`, `PrepareForOrdinaryCall`, and
`FunctionDeclarationInstantiation` require a logically fresh activation and
the prescribed parameter, `arguments`, var, lexical, and closure semantics.
Assignment evaluation captures a Reference Record before evaluating the
right-hand side; `PutValue` later writes through that captured record.

The optimization may therefore retain and use the already-resolved
Environment Record. It must not redirect the write by resolving the identifier
again after the right-hand side runs. It must also preserve temporal dead zone,
constant, immutable, function-name, global-object, import, `with`, and deleted
binding behavior.

## Design

When `put_value_by_ref` receives `IdentifierRef::SpecificEnv`, first attempt a
single mutable lookup in that exact Environment Record. If the record is not a
global environment and the binding is still present:

- initialized `var` and `let` bindings are written directly;
- uninitialized lexical bindings report the existing TDZ error;
- initialized `const` bindings report the existing assignment error;
- function-name and immutable bindings retain their strict/sloppy behavior.

If the record is global, the binding is indirect, or the binding disappeared
while the right-hand side was evaluated, retain the existing general path.
That path handles global object mirroring and proxy behavior, module imports,
and unresolvable-reference semantics.

This is a storage-level shortcut only. It does not omit an Environment Record,
change reference capture timing, cache a mutable borrow across user code, or
specialize based on function hotness.

## Alternatives considered

Slot-indexed function frames could remove most string hashing and scope-chain
walking, including for straight-line code, but require parser/body metadata,
new binding storage, eval and closure deoptimization rules, and a wider
correctness surface.

Adding `#[inline]` to indirect-binding checks is smaller, but only removes one
cross-module call boundary and leaves repeated map probes on every assignment.

Reusing a cached environment location at each AST identifier site would help
loops after warmup, but not the motivating one-shot sites and would require
invalidation for `eval` and `with`.

The resolved-record write is the smallest measured vertical slice: it removes
redundant work for the translated-C register assignments that dominate the
profile while preserving the existing slow path for observable cases.

## Verification

- Add focused coverage for mutable local writes, TDZ and constant failures,
  sloppy and strict named-function bindings, closure reference capture across
  right-hand-side calls, globals, imports, and `with`.
- Compare cold Mandreel setup wall time and a `perf` profile before and after.
- Run the language assignment, function-code, and environment-sensitive
  test262 areas, followed by the full suite against the `origin/main`
  baseline.
- Run formatting, Clippy, the release build, and release tests.
