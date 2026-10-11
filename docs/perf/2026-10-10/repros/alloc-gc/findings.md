# alloc-gc — allocation rate and garbage-collection cost

jsse HEAD `2b621c5`, release build on buildbox3, 2026-10-10. Repros: `bench_alloc-gc_*.js` in this directory; each header carries its measured numbers and each prints a result line identical on jsse and boa.

## Summary

- **Only splay is GC-bound.** About 58% of its timed window (4.1 of 7.0 s) goes to 14 full collections of a ~770k-object heap, at 275 ms each.
- **Cause: the pacer, not the generational design.** After 2 minors with ≥90% nursery survival, `GcPacer` forces a major every 8192 allocations (`gc.rs:104-110`, `:169-180`), bypassing the live-set-proportional budget (`:193-202`).
- **A/B of removing that rule (plain release, load 5-8):** splay timed result 6951-7257 → 4116-4578 ms (-40%), majors 48 → 8, RSS 1.34 → 1.67 GB.
- **Minors are generational:** 2.2 ms per 8192-object nursery whether 44k or 770k objects are live.
- **hash-map (GC 0.7%), early-boyer (4%) and ai-astar (13%) are not allocation/GC gaps.** They are tree-walker bails (see "Did not reproduce").
- **Allocation is expensive everywhere:** `JsObjectData` is 856 B; `{}` costs 2 mallocs, `{a,b}` costs 7; a 10-element array literal costs 34 mallocs and stores every element twice.
- **Map/Set are linear scans** with a whole-Vec clone on `get`/`has`: 6-7x slower than boa at 8000 keys.

## Gap table

Median of 5 runs, parity of the result line verified.

| repro | jsse s | boa s | ratio | jsse --no-bytecode s | loadavg |
|---|---|---|---|---|---|
| splay_mini | 2.63 | 0.67 | 3.93 | 2.88 | 4.7 |
| native_map (8000 keys) | 3.48 | 0.52 | 6.72 | 3.48 | 4.4 |
| map_rolling | 3.43 | 0.57 | 6.05 | 3.48 | 4.6 |
| hashmap_js (JetStream source, COUNT 60000) | 5.64 | 2.07 | 2.72 | 6.79 | 4.8 |
| array_literal | 2.17 | 0.82 | 2.66 | 2.12 | 3.3 |
| retain_grow | 6.54 | 3.03 | 2.16 | 6.54 | 4.5 |
| liveset (control, GC ~10%) | 5.84 | 3.33 | 1.75 | 6.34 | 5.3 |
| objlit_churn | 4.08 | 2.53 | 1.62 | 4.18 | 4.5 |
| ctor_churn | 4.18 | 2.57 | 1.62 | 4.53 | 4.1 |

## Root causes (ranked)

1. **Pacer forces a full collection per nursery under high survival.**
   - Evidence: the splay collection sequence is `M m m M m m …` for all 160 collections. Majors average 115 ms during setup and 275 ms at the plateau.
   - Mechanism: the cycle repeats every 24,576 allocations regardless of heap size.
   - Validators: 4-iteration ai-astar goes 4.5 → 4.0-4.4 s/iter with RSS unchanged at 719 MB; hash-map time is unchanged and RSS goes 136 → 185 MB.
   - Rationale of the existing rule: commit `47c1754` / `docs/specs/2026-07-20-generational-gc-nursery-design.md` guard against "survivor-heavy workloads turning minors into near-major traces". `remembered_set_is_dense` (`gc.rs:638`) already covers that case.
2. **Per-object footprint.**
   - Sizes: `JsObjectData` 856 B, `PropertyMap` 304 B, `ObjectKind` 192 B, `JsFunction` 120 B (`types.rs:1984`).
   - `class_name`: `"Object".to_string()` at `types.rs:2280` costs a malloc/free per object. 258 read sites; `== "WeakMap"` compares sit in GC hot loops (`gc.rs:583`, `:855`, `:909`).
   - Major cost: ~330 ns per live object. A major makes four passes over each object: mark, ephemeron loop (`:846`), sweep (`:885`), tenure (`:899`). Per-pass cache-miss split is UNVERIFIED (`perf` unavailable).
   - `free_gc_object` (`:799`) does 7 SipHash `remove`s per freed object.
3. **Array literals store every element twice.**
   - `create_array_with_holes` (`array.rs:3505`) calls `insert_value(i.to_string(), …)` for each element and also pushes into `array_elements`.
   - The property map spills to a SipHash `HashMap` at element 5. The dense-without-mirror form already exists (`types.rs:2338`).
   - splay_mini callgrind: `eval_array_literal` is 29% of all instructions.
4. **Property insertion path.**
   - `insert_value` (`types.rs:3339`) does intern-hash, `contains_key`, `record_property_creation` and `insert`.
   - Object-literal keys are rebuilt per evaluation (`literals.rs:1328`); `property_order` is a second Vec of Arc clones.
   - SipHash is ~14-18% of instructions in splay_mini and ctor_churn.
5. **Map/Set.**
   - `collections.rs:216`, `:269`, `:960` clone the whole entry Vec per `get`/`has`. `set`/`add`/`delete` are linear and `size` is O(n). Tombstones are never compacted. Spec 24.1 requires sublinear access.
6. **No card marking.** A remembered old object is re-traced whole (`gc.rs:668-693`). In retain_grow, minors cost 15 ms each versus 2.2 ms for a normal nursery.

## Proposed work items

- **WI-1 `perf(gc): stop forcing a major per nursery under high survival`** (S).
  - Scope: `GcPacer::end_minor_collection` / `charge_object`. Options: drop the rule, make the probe respect `major_threshold_bytes`, or scale the probe with live set.
  - Acceptance: splay timed ≤ 4.5 s, majors ≤ 12, ai-astar/hash-map RSS ≤ +40%, test262 and `JSSE_GC_STRESS=7` green.
  - Splay is still 3.6x boa afterwards; the rest is mutator cost.
- **WI-2 `perf(object): shrink JsObjectData`** (L, split):
  - 2a: `class_name` → static str or id.
  - 2b: box `callable`, `private_fields`, `class_instance_field_defs`, `gc_native_roots`, and the large `ObjectKind` variants.
  - 2c: fold the major passes.
  - 2d: skip empty side-table removes in `free_gc_object`.
  - Acceptance: `size_of::<JsObjectData>() <= 320`, ≤ 120 ns per live object per major, splay RSS ≤ 0.9 GB.
- **WI-3 `perf(array): dense array literals without property-map mirror`** (S-M).
  - Acceptance: array_literal ≤ 1.3x, ≤ 4 mallocs per `[0..9]`.
- **WI-4 `perf(object): cheaper property insertion and object literals`** (M).
  - Scope: cache interned keys per AST node, single-lookup insert, order stored inside `PropertyMap`, fast keyed hasher.
  - Acceptance: objlit_churn ≤ 1.4x, ctor_churn ≤ 1.3x.
- **WI-5 `perf(builtins): O(1) Map/Set with tombstone compaction`** (M).
  - Scope: hash index next to the ordered Vec, no `.cloned()`, live counter, compaction that keeps live-iterator positions correct, weak-collection/ephemeron code kept in sync.
  - Acceptance: native_map and map_rolling ≤ 1.0x, 100k keys in < 1 s.
- **WI-6 `perf(gc): per-card dirty tracking for large old arrays and maps`** (M-L, after WI-1).
  - Acceptance: retain_grow minor average ≤ 4 ms.
- **Cross-theme: bytecode compiler coverage.** It lacks object/array literals, function expressions, `let`/`const`, `switch`, `typeof`, `throw`, `instanceof`/`in` and `x.y++`; any function containing one runs in the tree-walker. The cost is ~98% of splay's AST work (`GeneratePayloadTree`) and is the dominant gap for hash-map and early-boyer.

## Did not reproduce

- **hash-map:** a hand-written JS HashMap, not built-in Map/Set. GC is 92 ms of 12.7 s. Bound by bytecode bails: `switch` (540k invocations), `typeof` (450k), `update target`, `throw`.
- **early-boyer:** GC is 4%. 2394 bodies bail on `instanceof`/`in`, 1679 on function expressions; `compile_ok` 46 versus `compile_bail` 4091.
- **ai-astar:** GC is 13% (11 majors). WI-1 removes about a third of that.
- **Short-lived churn (objlit_churn/ctor_churn):** GC ~3%. The gap there is call and property-store overhead.
- **Bytecode vs tree-walker:** `--no-bytecode` is within 0-20% of default on every repro, because allocating bodies bail anyway.
- **retain_grow:** the WI-1 experiment gains only 14% there (6.1-6.5 → 5.5 s). Mutator cost dominates.

## Reproduction commands

```
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
python3 $R/microbench.py '{"jsse":["'$R'/jsse-head/target/release/jsse"],"boa":["'$R'/boa-v0.22-src/target/release/boa"],"jsse_nobc":["'$R'/jsse-head/target/release/jsse","--no-bytecode"]}' <dir> out.json 5
# GC share: perf-counters build, run JetStream scripts generated from run-jetstream.py
# GC trace and WI-1 experiment: patched copies in $R/work-alloc-gc/src (JSSE_GC_TRACE, JSSE_GC_EXP_SUPPRESS)
valgrind --tool=dhat ...                                                         # mallocs per iteration
valgrind --tool=callgrind --toggle-collect='*gc_safepoint*' ...                  # mark cost
```

Scratch outputs (traces, dhat, callgrind) are in `$R/work-alloc-gc/` on buildbox3.
