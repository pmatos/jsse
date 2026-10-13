# #912 — per-card dirty tracking for large old Arrays

`before` is `main` at `0d41f4f0`; `after` is this branch. Release builds, `boa`
is boa 0.22 (`~/.cargo/bin/boa`). The host ran at load average 20-28 on 24
cores while measuring, so figures are medians of interleaved runs (wall and
user+sys CPU, 5 runs unless stated) and differences under ~10% are noise. Minor
averages come from the `perf-counters` build (`PERF gc_minor_us / gc_minor`,
new in this change), wall times from the default build.

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

| | before | after |
|---|---:|---:|
| minor collections | 67 | 67 |
| minor average (median of 9) | 5.60 ms | 2.84 ms |
| CPU time | 1.861 s | 1.287 s |
| boa 0.22 CPU time | 2.402 s | |
| after / boa | | 0.54x |

Acceptance is a minor average at or under 4 ms: met (the 15 ms in the issue is
the slower reference box; the same run on this host measured 5.6 ms before).
Intermediate steps on the same repro, measured under heavier load (average
minor): 9.9 ms before, 8.4 ms with cards plus the young bitmap, 4.9 ms with the
promotion change as well.

## No regression

| repro | before | after | after / before |
|---|---:|---:|---:|
| splay_mini (CPU) | 0.455 s | 0.451 s | 0.99 |
| Octane splay score (median of 11, spread 729-1468) | 1236 | 1180 | noise |
| Octane richards score (median of 11) | 146 | 145 | 0.99 |
| bench_alloc-gc_liveset (wall) | 0.797 s | 0.797 s | 1.00 |
| bench_alloc-gc_map_rolling | 0.059 s | 0.053 s | 0.90 |
| bench_alloc-gc_hashmap_js | 0.877 s | 0.734 s | 0.84 |
| bench_alloc-gc_objlit_churn (median of 9) | 0.529 s | 0.555 s | 1.05 (noise) |
| bench_alloc-gc_ctor_churn (median of 9) | 0.614 s | 0.594 s | 0.97 |
| bench_alloc-gc_array_literal | 0.136 s | 0.124 s | 0.91 |

Every checksum matched across before, after and boa.

## Not done

Map/Set entry vectors still use the object-level barrier (the whole entry list is
re-scanned when a remembered Map is). `KeyedEntries` compaction moves entries, so
carding it needs the same "any structural change dirties everything" rule; it is
the natural follow-up.
