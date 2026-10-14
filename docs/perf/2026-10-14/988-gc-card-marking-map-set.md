# #988 — per-card dirty tracking for Map/Set entry lists

Follow-up to #912 (`docs/perf/2026-10-13/912-gc-card-marking.md`). `before` is
`main` at `f9504bbe`; `after` is this branch. Release builds. Minor-collection
figures come from the `perf-counters` builds (`PERF gc_minor_us / gc_minor`),
medians of 5 interleaved runs; CPU times (user+sys) come from the default
builds, medians of 5 interleaved runs; boa is boa 0.22 (`~/.cargo/bin/boa`).
The laptop was at load average 20-30 throughout (other users' jobs), so wall and
CPU time swing by +-15% between identical runs: treat differences under ~15% as
noise. The minor-collection totals are the cleaner signal.

## What changed

* `KeyedEntries` (the [[MapData]]/[[SetData]] storage) carries a dirty-card
  bitmap over its physical slots, 512 slots per card (`CARD_ELEMENTS`, 4 in
  debug builds). `None` means every card is dirty, the state of every new
  collection and of any collection after a structural change: a plain `upsert`
  (an edge that names no slot), `compact` (slots move), `clear`. `remove`
  adds no edge and keeps the bitmap unless it triggers a compaction. A copy
  (`Clone`) starts all-dirty.
* `Map.prototype.set`, `getOrInsert`, `getOrInsertComputed` and
  `Set.prototype.add` (the Map/Set constructors and `groupBy` reach the same
  paths or the conservative one) run `Interpreter::gc_write_barrier_entry`, which
  queues an old owner only when an entry references a nursery object, then store
  through `KeyedEntries::upsert_marking`, which marks just the slot's card.
* A minor collection scans only the dirty cards of a strong Map/Set and keeps
  exactly the cards that still hold a young reference, like the Array path
  (`scan_cards` is now shared). `JsObjectData::has_card_tracking` (Array,
  strong Map, strong Set) replaces `array_data().is_none()` as "scan the
  non-bulk fields unless the barrier says only a card changed". WeakMap and
  WeakSet are not carded (they are re-scanned whole through the ephemeron path,
  as before).
* The debug-build barrier check after marking now also covers strong Map/Set
  entries.

## Minor collections (perf-counters builds, median of 5)

Large old collections whose new entries hold fresh objects:

| script | minors | before total | after total | before avg | after avg |
|---|---:|---:|---:|---:|---:|
| mapgrow (400k-entry Map and Set, `set`/`add` of fresh objects) | 92 | 323.8 ms | 167.7 ms | 3.52 ms | 1.82 ms |
| mapwindow (300k-entry Map, append + delete oldest, 600k rounds) | 104 | 245.5 ms | 140.1 ms | 2.36 ms | 1.35 ms |
| mapretain (250k-entry Map, 350k random overwrites, then 200k-member Set) | 92 | 367.5 ms | 297.7 ms | 3.99 ms | 3.24 ms |

mapgrow and mapwindow write where the collection grows, so only the tail cards
are dirty per minor. mapretain overwrites uniformly at random (about 8k stores
per minor over ~490 cards), so every card is dirty each minor and cards alone
skip nothing; what it gains is the tighter slot loop and the retention rule.

## JetStream drivers (the issue's named targets)

`hash-map` and `lazy-collections`, one iteration of the runner's driver. They
have few minor collections (8 and 3), so the collector is a small share of
either:

| driver | minors | minor total before | after | CPU before | CPU after | boa CPU |
|---|---:|---:|---:|---:|---:|---:|
| hash-map | 8 | 12.6 ms | 14.3 ms | 2.034 s | 2.074 s | 1.700 s |
| lazy-collections | 3 | 25.6 ms | 24.4 ms | 1.018 s | 1.052 s | 0.510 s |

Both are neutral within noise: neither keeps a large old Map/Set that
accumulates young references across many minors (hash-map's keys are strings
and numbers; lazy-collections makes no Map/Set calls on its hot path, see
#907). This change does not move them, and does not regress them.

## CPU time (default builds, median of 5, interleaved)

| script | before | after | boa |
|---|---:|---:|---:|
| mapgrow | 0.805 s | 0.873 s | 1.256 s |
| mapwindow | 1.084 s | 1.030 s | 377.3 s |
| mapretain | 1.233 s | 1.257 s | 1.903 s |

The minor-collection saving (100-160 ms of 0.8-1.1 s) is below the run-to-run
spread at this load, so the CPU table is a no-regression check, not a speedup
claim. Checksums matched across before, after and boa for all three.

## Not done

Weak containers (WeakMap/WeakSet) and FinalizationRegistry cells keep the
conservative whole-object scan. Compaction resets the bitmap instead of
remapping it, so the first minor after a compaction scans every card of that
collection once.
