# #912 — per-card dirty tracking for large old Arrays

`before` is `main` at `0d41f4f0`; `after` is this branch. Release builds. Minor
averages come from the `perf-counters` build (`PERF gc_minor_us / gc_minor`, new
in this change; the before build carries only that counter patch); CPU times
(user+sys) come from the default builds. Primary numbers are from buildbox3
(AMD EPYC 7501, load average 2-5 while measuring), medians of 5 interleaved
runs (7 for the minor average); boa is boa 0.22.0
(`~/dev/bench-workspace/boa-0.22.0/boa`). A laptop run at load average 20-28
is kept as a cross-check. Differences under ~10% are noise.

## What changed

* An old Array's element vector has a dirty-card bitmap (512 elements per card;
  4 in debug builds so unit tests and stress runs cross card boundaries).
  `array_elements_mut()` — the only general mutable accessor — resets it to
  "every card dirty", so an operation that moves or rewrites elements cannot
  leave a stale bitmap. A single-slot store that ran
  `gc_write_barrier_element` first (indexed store, tree-walker overwrite and
  append, `Array.prototype.push`) marks just its card.
* A minor collection scans only the dirty cards of a card-tracked Array and keeps
  exactly the cards that still hold a young reference; a full scan converts an
  untracked Array into a bitmap.
* Two supporting changes were needed to reach the target, because the repro
  writes uniformly at random (8,192 stores per minor over 489 cards, so every
  card is dirty each minor, so cards alone skip nothing):
  * a per-minor `YoungIds` bitmap of the nursery, so "is this element young"
    is one bit rather than a load of the referent's header (the old scan took a
    cache miss per element, twice);
  * promotion no longer remembers every promoted object: only one whose tracing
    saw a young child, or whose kind has edges the tracer cannot see. In
    `retain_grow` ~7,800 promoted records per minor were being re-scanned for
    nothing.

## retain_grow (the issue's acceptance criterion)

buildbox3, 67 minor collections in both builds:

| | before | after |
|---|---:|---:|
| minor average (median of 7) | 9.06 ms | 3.04 ms |
| same script with a 2,000-slot table (normal nursery, no large array) | 0.76 ms | 0.80 ms |
| CPU time | 2.066 s | 1.740 s |
| boa 0.22 CPU time | 2.995 s | |
| after / boa | | 0.58x |

The acceptance bar is a minor average at or under 4 ms: met on the reference box
(3.04 ms). The remaining time is mostly tracing the ~16k surviving young
objects per minor (a scan of the table itself is now a small part), which is
the cost of the workload's ~95% nursery survival, not of the remembered set.
(The 15 ms in the issue was measured on an older HEAD; before is 9.06 ms at
`0d41f4f0`.) On the laptop, load 20-28: 5.60 ms before, 2.84 ms after. The
intermediate steps there, measured under heavier load (average minor): 9.9 ms
before, 8.4 ms with cards plus the young bitmap, 4.9 ms with the promotion change
as well.

## No regression (buildbox3, CPU time, median of 5)

| repro | before | after | after / before | boa |
|---|---:|---:|---:|---:|
| splay_mini | 0.785 s | 0.795 s | 1.01 | 0.653 s |
| Octane splay, fixed work (SplaySetup, 150 x SplayRun, SplayTearDown) | 6.120 s | 6.132 s | 1.00 | fails |
| Octane richards, fixed work (200 x runRichards) | 15.279 s | 15.529 s | 1.02 | fails |
| bench_alloc-gc_liveset | 2.088 s | 2.072 s | 0.99 | 3.293 s |

boa 0.22 throws on the two Octane drivers, so there is no boa figure for them.
The retain_grow, splay_mini and liveset checksums matched across before, after
and boa. Splay is unchanged because its nodes are objects, not Array elements;
what helps it is the promotion change, whose effect is below the noise floor.

Laptop cross-check (wall, load 20-28): liveset 1.00, map_rolling 0.90, hashmap_js
0.84, objlit_churn 1.05 (noise), ctor_churn 0.97, array_literal 0.91, Octane
splay score median of 11 1236 vs 1180 (spread 729-1468, uninformative), richards
146 vs 145.

## Not done

Map/Set entry vectors still use the object-level barrier (the whole entry list is
re-scanned when a remembered Map is). `KeyedEntries` compaction moves entries, so
carding it needs the same "any structural change dirties everything" rule; it is
the natural follow-up.
