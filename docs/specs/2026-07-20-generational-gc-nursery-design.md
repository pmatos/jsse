# Generational GC Nursery

## Problem

JSSE's collector runs a complete mark and sweep whenever allocation pressure
requests a collection. Its adaptive threshold controls collection frequency,
but every collection still traces the complete live object graph. On the
current arena-backed baseline, `ai-astar` still averages about 1.52 seconds per
iteration while retaining about 1.0 GiB.

The arena gives objects stable numeric IDs and exposes stable owned handles. A
nursery must preserve those identities, the existing temporary-root contract,
and the ephemeron behavior used by WeakMap and WeakSet.

## Approaches Considered

1. Add a non-moving nursery to the existing arena, promote survivors after two
   minor collections, and use a remembered set for old-to-young references.
   This retains stable IDs and confines routine collection work to the nursery,
   roots, and old objects mutated since the previous minor collection. This is
   the selected approach.
2. Add a copying nursery and rewrite every reference when survivors move. This
   has excellent allocation locality, but JSSE's object IDs and independently
   cloned handles are used throughout the interpreter. Rewriting all IDs,
   caches, environments, scheduler state, and native captures would be a much
   larger and riskier representation change.
3. Sweep only recently allocated arena slots without a write barrier. This is
   small, but cannot distinguish unreachable young objects from objects stored
   in an old object's property or internal slot. Scanning every old object to
   recover those edges restores the original O(live) cost.

## Design

Store collector-only metadata beside each arena payload in `ObjectAllocation`:
generation, nursery age, mark state, and remembered-set membership. Mutable
borrows of an old object form a conservative write barrier and enqueue its
stable ID once. Hot property, array, and collection writes use a precise
value-aware barrier before an untracked mutable borrow, avoiding remembered-set
pollution when a mutation only stores a primitive or old object. Other direct
mutations retain the conservative barrier.

Some object kinds expose mutable environments through reference-counted
handles: user functions, iterators, arguments objects, and module namespaces.
Those old objects remain conservatively remembered even when a scan currently
finds no young edge. This covers environment assignments that occur without a
second mutable borrow of the owning object. Other remembered objects remain in
the set only while they contain a young strong or weak edge.

New objects enter a 4 MiB nursery, represented as a list of stable arena IDs.
A minor collection:

1. gathers the existing interpreter roots;
2. traces young roots and the young descendants of remembered old objects;
3. computes the WeakMap ephemeron fixpoint for live young or remembered old
   WeakMaps;
4. clears dead young WeakMap keys and WeakSet members;
5. frees unmarked nursery objects, ages survivors, and promotes objects that
   survive two minor collections.

Minor marking uses a bit stored on each nursery cell, avoiding allocation or
clearing a heap-capacity-sized mark bitmap. Promoted objects enter the
remembered set once so any still-young descendants are discovered on the next
minor collection.

Full collections retain the current complete mark, ephemeron, weak cleanup,
and sweep algorithm. Explicit `$262.gc()` requests a full collection. Major
allocation debt is tracked separately from the nursery and is not reset by a
minor collection. After a full collection, the next major budget permits one
additional live-set worth of allocation debt, with an 8 MiB floor. External
ArrayBuffer pressure continues to count toward the major budget, and
backing-store bytes are released whether their owner dies in a minor or full
collection.

Two consecutive minors with at least 90% survival suppress further scavenges;
the next 4 MiB allocation budget requests a major collection instead. The same
fallback applies when the remembered set covers at least 75% of a heap with
8,192 or more live objects. These guards keep a mutation-heavy or
survivor-heavy workload from turning minor collection into repeated near-major
traces.

## Specification Semantics

ECMAScript does not require that unreachable objects be collected, and gives
implementations latitude to approximate liveness conservatively. It does
require weak collections not to make their keys or members strongly live. The
minor collector therefore preserves the existing reachability approximation,
including WeakMap's conditional value reachability, while deferring reclamation
of unreachable old objects to a full collection.

## Validation

Add unit coverage for nursery allocation, promotion, remembered-set
deduplication, old-to-young retention, unreachable-young reclamation, minor and
major pacing, and explicit full collection. Run the existing custom GC tests,
the WeakMap and WeakSet test262 areas, and the full test262 suite. Rebuild in
release mode and compare identical multi-iteration `ai-astar` and `hash-map`
runs against the same `origin/main` build, recording both wall time and peak
resident memory.
