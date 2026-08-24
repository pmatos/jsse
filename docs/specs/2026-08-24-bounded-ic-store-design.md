# Bounded Per-Body Inline-Cache Store

## Problem

`IcStore` owns one `BodyIcStore` for every Body the interpreter has executed.
Each store pins the Body's statement `Rc`, preserving the ABA-safety contract of
the pointer key but retaining short-lived `Function`-constructor and `eval`
syntax forever. The hoisting-analysis cache has an independent bound, so the IC
store is now the remaining unbounded owner in Body-churning workloads.

The store cannot evict by removing from its `Vec`: `BodyStoreHandle` values live
in `Interpreter::current_ic_handle` and in saved parent handles while nested
Bodies execute. Renumbering entries or reusing an occupied index could make a
live evaluator access another Body's IC slots.

## Approaches Considered

1. Bound `IcStore` in place with active-use tracking, stable reusable slots, and
   generation-tagged handles. This is selected because it preserves the narrow
   AST/runtime seam from ADR 0001 and confines the lifetime policy to the store.
2. Merge hoist analyses into `BodyIcStore`. This would remove one Body-keyed map
   and one pin, but the combined cache would still need the same active-handle
   eviction machinery. It also couples payloads with different lookup and use
   patterns, making this fix broader without removing its hard part.
3. Put the runtime memo on `Body`. This makes cache lifetime follow AST lifetime
   naturally, but moves interpreter cache types back into the AST, reversing
   ADR 0001, and requires changes across parser, generator transform, `eval`, and
   dynamic-function construction paths.

## Design

Use an 8,192-entry default capacity, matching the bounded hoist cache. Store
entries in stable slots. Each slot has a generation and an optional entry; an
entry contains the `BodyIcStore`, its recency tick, and an in-flight use count.
The pointer-keyed index maps a Body to a slot, while a free list supplies evicted
slots before the backing vector grows. Reusing a slot increments its generation,
so a stale handle fails validation rather than aliasing a different Body.

Entering a Body obtains or creates its entry, increments its in-flight count,
and installs its generation-tagged handle as `current_ic_handle`. The saved
parent handle remains counted as in flight while a nested Body runs. Leaving the
Body decrements the installed handle before restoring its parent. Recursive
entry into the same Body increments the same count.

On a full-cache miss, evict the older half of inactive entries in one sweep.
Bulk eviction amortizes the scan across subsequent insertions, while recency
keeps hot stores warm. An active entry is never eligible. If every entry is
active, the store may temporarily exceed its nominal capacity because retaining
correct handles takes precedence; when an over-capacity entry becomes inactive
on return, evict that entry immediately. Consequently retained entries are
bounded by the greater of the configured capacity and the maximum number of
simultaneously active distinct Bodies.

Eviction takes the entry from its slot, derives the key from the still-pinned
Body, removes that exact key-to-slot mapping, and only then drops the Body pin
and releases the slot to the free list. Every surviving pointer key therefore
continues to name a live pinned Body, preserving the ABA invariant.

The IC store remains a pure optimization. Calling a Body after eviction creates
empty call/property slots of the Body's assigned sizes and re-warms them; no
ECMAScript-visible state is lost.

## Validation

Add focused tests that demonstrate the old unbounded retention, then cover the
capacity bound, release of evicted Body pins, slot reuse, stale-generation
detection, identity sharing, active-entry protection, nested same-Body use, and
temporary overflow cleanup. Add an interpreter integration test that executes
more than 8,192 distinct dynamic-function Bodies through the normal call path,
checks results, and verifies both entry and slot counts plateau.

Run the Function-constructor test262 area because `CreateDynamicFunction` is the
observable source of the churning Bodies, then run all repository quality gates
and the full default test262 suite. Since this is cache lifetime policy,
conformance pass counts should not change.
