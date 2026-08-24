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
2. Keep a fixed-capacity map plus insertion-order queue and evict one oldest
   entry on a full-cache miss. This keeps lookup and insertion O(1), but a scan
   across more Bodies than the capacity repeatedly evicts hot analyses along
   with cold ones.
3. Track a recency tick per entry and discard the least-recently-used half in
   one sweep when the cache fills. This is selected: hits preserve hot analyses,
   and bulk eviction amortizes the O(n) selection pass across many insertions.

## Design

Introduce a small `HoistCache` wrapper containing the existing pointer-keyed map
and a monotonic recency clock. Its capacity is 8,192 entries: large enough to
avoid churn for ordinary programs with many static Bodies while still placing a
fixed upper bound on body-churning `Function` and `eval` workloads.

The bound is an entry count, and an entry costs its own few hundred bytes plus
whatever Body it pins, so it caps retention rather than naming a byte figure.
`IcStore` independently pins the same Bodies, so this cache's bound alone did
not make a churning workload level off. That table is now bounded separately by
#468; both bounds are required before a dead Body can be released.

On a hit, the wrapper updates the entry's recency tick and returns the cached
analysis. On a miss at capacity, it selects the median recency tick and keeps the
newer half, so the most recently used entry always survives a sweep, then inserts
the new analysis. Every retained entry continues to pin its Body, preserving ABA
safety. Removing an entry drops that pin; a later call of the evicted Body
recomputes and safely reinserts its analysis.

The wrapper owns the key, the recency metadata, the pin, and the collection
itself, so the evaluator cannot update part of an entry or pair a key with a
different analysis. It exposes one `analysis_for(&Body)` entry point — taking the
Body, whose `key` method both Body-keyed side tables share — so the evaluator
neither derives the key, nor decides when to evict, nor supplies the analysis. No
ECMAScript-visible behavior changes.

## Validation

Add focused cache tests for identity memoization, hit accounting, the capacity
bound, release of an evicted Body's pin, recency-aware eviction, and a sweep at
the capacity floor (where a median cutoff that dropped its own tick would evict
the newest entry).
Add interpreter integration tests that execute more than 8,192 distinct
`Function` constructor Bodies through the normal call path and verify both the
computed result and cache occupancy, plus repeated calls that must keep hitting
one cached analysis. Run the Function constructor and function-statement test262
areas, followed by the repository formatting, linting, release build/tests, and
full test262 gate.
