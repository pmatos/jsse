# Bounded Hoisting-Analysis Cache

## Problem

JSSE caches the declaration-name analysis used when executing a Body. The cache
is keyed by the Body's `Rc` identity, and each `HoistAnalysis` owns a clone of
that `Rc` so a freed Body address cannot be reused for an unrelated entry. This
ABA-safety invariant also means the interpreter-global cache retains every Body
it has seen. Repeatedly creating and calling distinct dynamic functions therefore
grows the cache without bound.

The cache is only an optimization. `FunctionDeclarationInstantiation` still
requires declaration bindings to be instantiated for every call, and an evicted
Body must transparently fall back to collecting the same names again.

## Approaches Considered

1. Store weak Body references and discard dead entries. This follows object
   lifetime closely, but it makes pointer reuse and entry validation more subtle
   and still permits unbounded growth while many distinct functions remain live.
2. Clear the entire map when it reaches a fixed limit, matching the compiled
   RegExp cache. This is very small, but a single miss discards every useful
   analysis and can cause avoidable re-analysis cliffs.
3. Keep a fixed-capacity map plus insertion-order queue and evict one oldest
   entry on a full-cache miss. This is selected: lookup and insertion remain
   O(1), eviction is deterministic, and no dependency or hit-path scan is added.

## Design

Introduce a small `HoistCache` wrapper containing the existing pointer-keyed map
and a FIFO queue of its keys. Its capacity is 256 entries, consistent with the
existing compiled RegExp cache bound.

On a hit, the wrapper clones and returns the cached analysis without changing
order. On a miss at capacity, it removes the oldest key from both collections
before inserting the new analysis. Every retained entry continues to own its
Body `Rc`, preserving ABA safety. Removing an entry drops that ownership; a
later call of the evicted Body recomputes and safely reinserts its analysis.

The wrapper owns the map/queue synchronization invariant so the evaluator cannot
update one without the other. No ECMAScript-visible behavior changes.

## Validation

Add an interpreter integration test that executes more than 256 distinct
`Function` constructor Bodies through the normal call path, calls the oldest
function again after eviction, and checks both its result and the cache bound.
Run the Function constructor and function-statement test262 areas, followed by
the repository formatting, linting, release build/tests, and full test262 gate.
