# Try clause continuation states

## Problem

The async-function transformer lets a final statement reuse the continuation
state supplied by its enclosing construct. This is intentional for transformed
loops: a `for-of` used as the last statement in a block sets the transform's
current state to its own `after_state`.

`transform_try_statement` currently finalizes the current state
unconditionally after transforming a try block or catch body. When the last
statement is a transformed `for-of`, the current state is already the caller's
continuation. Finalizing it replaces the continuation's existing terminator.
For a try nested directly in an outer loop body, this replaces the outer
`ForOfHead` with a self-directed `Goto`, so execution never enters the loop.

The same alias is possible at catch and finally clause boundaries. A finalizer
also needs a `TryExit` terminator, so merely skipping its finalization would
bypass completion restoration.

## Design

Give each try clause an explicit normal-completion target:

- A try block or catch body with a finalizer targets the finalizer entry state.
- A try block or catch body without a finalizer targets the caller's existing
  continuation state.
- Clause finalization emits a `Goto` only when transformation did not already
  land on that target. This preserves a reused continuation's terminator.
- A finalizer targets a new private exit state. That exit state owns the
  `TryExit` terminator that restores a pending completion or continues to the
  caller's state.

This is a transformer-only change. Runtime loop, iterator, and handler stacks
retain their existing behavior.

## Alternatives considered

1. Allocate bridge states around every try and catch clause. This prevents
   aliasing but adds states even when the existing continuation is safe, and it
   changes more generated control flow than the defect requires.
2. Make `finalize_current_state` reject every attempt to replace a terminator.
   This would enforce a broad invariant at one seam, but the transformer also
   creates placeholder states that are intentionally finalized later, such as
   finalizer entry and function completion states.
3. Guard only the exact catch-free reproduction. This is the smallest diff,
   but leaves the identical alias at catch and finally boundaries.

## Validation

Add a `test262-extra` async regression whose try block ends in a transformed
`for-of`, matching issue #492. Include catch-final and finally-final variants
to cover the shared clause-boundary invariant. Each promise must resolve to the
expected value; before the fix the generated outer loop head is overwritten
and the test times out.

Run that regression with a short timeout, then the upstream async-function,
`for-of`, and `try` directories, the custom suite, the Rust quality gate, and
the full test262 suite.
