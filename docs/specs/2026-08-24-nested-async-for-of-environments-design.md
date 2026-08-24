# Nested async `for-of` environments

## Problem

The async-function state-machine driver represents every active `for-of`
loop in `for_of_stack`, but represents the current lexical iteration
environment in one `for_of_iter_env` slot. When a transformed `for-of` is
nested inside another transformed `for-of`, the inner head takes and disposes
that singleton slot. Its new iteration environment is then parented directly
to the function environment. The outer loop's per-iteration binding and any
lexical declarations stored in that environment are therefore unreachable
before and after an `await` in the inner body.

This differs from ECMA-262 `ForIn/OfBodyEvaluation`. Every lexical `for-of`
iteration creates an iteration environment whose outer environment is the
environment active when that loop began. Nested loops consequently form an
environment chain, and `Await` resumes the execution context with that chain
intact.

## Design

Replace the async driver's tuple-based active-loop metadata and singleton
iteration environment with one structured entry per active `for-of`. Each
entry owns:

- the generated iterator variable and control-flow states used by the state
  machine;
- the loop's outer lexical environment, corresponding to the algorithm's
  `oldEnv`; and
- the current iteration environment when the loop has a lexical binding.

The effective environment for a state is the innermost active loop's current
iteration environment, falling back to that loop's outer environment and then
the function environment. `ForOfInit` captures the effective environment as
the new loop's outer environment. `ForOfHead` disposes only its own previous
iteration environment. A lexical head creates its next environment with the
saved outer environment as parent; a `var` or assignment head continues in
the saved outer environment. Finishing or breaking a loop removes its entry,
revealing the enclosing loop environment again.

The entire entry stack is stored in `AsyncFunctionState` at suspension, so an
`await` preserves every active loop environment. GC traces the outer and
iteration environments held by every entry.

`ForOfInit` will also evaluate its iterable in the effective environment. For
a lexical loop head, it will create a temporary child environment containing
the uninitialized head bindings while evaluating the iterable, implementing
the head-evaluation TDZ without mutating the function environment. This is
required for nested RHS expressions to see outer-loop bindings while still
shadowing same-named head bindings.

## Alternatives considered

1. Keep the singleton and infer its parent when entering or leaving a loop.
   This is a smaller type change, but it cannot reliably distinguish a loop's
   saved outer environment from its own current iteration environment across
   nesting, `var` heads, suspension, `break`, and `continue`.
2. Rewrite nested loops into closures or replay the surrounding source after
   each suspension. That would make source scopes implicit in the transform,
   but substantially expands the state-machine rewrite and risks changing
   iterator-close and abrupt-completion behavior.
3. Store only a vector of environments parallel to the existing tuple stack.
   This avoids a new struct but permits the control-flow and environment stacks
   to drift apart. One structured stack makes the invariant type-visible.

## Abrupt completion and resource disposal

Existing iterator-close behavior remains attached to the innermost active
loop. Normal exhaustion and `break` dispose that loop's current iteration
environment before removing the entry. `continue` retains the entry and lets
the next `ForOfHead` dispose the completed iteration. Function return closes
active iterators and disposes active iteration environments from inner to
outer before settling the async function.

Any disposal error follows the driver's existing pending-exception path; an
uncatchable host exit continues to propagate immediately.

## Validation

Add a `test262-extra` async regression covering:

- outer `const` and a lexical declaration in its body across an inner
  `for-of` suspension;
- multiple outer and inner iterations, proving fresh per-iteration bindings;
- an inner iterable expression that reads the outer binding;
- a `var` inner head, proving nesting does not depend on the inner head being
  lexical; and
- the same nested shape under module top-level await, which previously raised
  a spurious TDZ error.

Run the focused regression, relevant upstream async-function and `for-of`
test262 directories, the custom suite, and the repository's full quality gate.

