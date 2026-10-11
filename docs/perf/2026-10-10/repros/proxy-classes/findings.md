# proxy-classes — Proxy/Reflect, class construction, private fields, Map/WeakMap

jsse HEAD `2b621c5` vs boa 0.22.0, buildbox3, 2026-10-10. 19 repros: `bench_proxy-classes_*.js` in this directory (each prints a checksum line identical on jsse, boa and node). Raw data on buildbox3: `$R/work-proxy-classes/{micro-proxy-classes.json,cg,counters}`.

## Summary

Most of the proxy and class gap is generic interpreter overhead, not Proxy or class machinery:
- Class construction costs the same as function construction (799M vs 798M instructions).
- Private fields are not slower than public ones (2.32 s vs 2.63 s).
- A proxy `get` trap adds only ~0.9k of 19k instructions per call.

The generic part is JS call cost (~13.5k instructions per call), SipHash identifier resolution (~18% of instructions in proxy-vue and proxy-mobx), and bytecode bails (~60-70% of function invocations stay on the tree-walker).

The one theme-specific outlier is **Map/Set/WeakMap/WeakSet lookup: 21-23x slower than boa**, because lookups clone the entry Vec and scan linearly.

## Gap table

5-run medians; `--no-bytecode` is within ~10-15% of default on every repro. The box was loaded (loadavg1 up to 28) for part of the run; ratios are interleaved with spread ≤3%, but re-measure absolute seconds on a quiet box before using them as acceptance baselines.

| repro | jsse s | boa s | ratio |
|---|---|---|---|
| weakmap_lookup (2000 keys) | 4.99 | 0.22 | 23.2x |
| map_lookup | 4.53 | 0.22 | 21.1x |
| map_iterate | 1.32 | 0.17 | 8.0x |
| proxy_set (Reflect.set, receiver = proxy) | 7.24 | 2.12 | 3.4x |
| proxy_vue_reactive | 5.09 | 1.52 | 3.3x |
| proxy_notrap (empty handler) | 5.74 | 1.77 | 3.2x |
| private_access | 2.32 | 0.77 | 3.0x |
| new_class_private_fields | 4.38 | 1.77 | 2.5x |
| proxy_get | 6.09 | 2.63 | 2.3x |
| new_function_ctor (control) | 3.38 | 1.37 | 2.5x |
| new_class_ctor | 3.38 | 1.47 | 2.3x |
| new_class_public_fields | 3.47 | 1.72 | 2.0x |

Whole JetStream workloads, single cold iteration (jsse vs boa): proxy-mobx 0.63 s vs 0.20 s, proxy-vue 1.14 s vs 0.35 s, raytrace-public-class-fields 11.5 s vs 5.4 s, raytrace-private-class-fields 14.7 s vs 6.4 s.

## Ranked work items

1. **`perf(collections): hash-indexed Map/Set/WeakMap/WeakSet; no entry-Vec clone`**
   - Cause: `get`, `has` and iterator `next` clone the whole entry Vec and then scan linearly. See `collections.rs:89`, `:216`, `:269`, `:813`, `:960`, `:1732`, `:1787`, `:2177`, and the storage type at `types.rs:2125`.
   - Evidence: WeakMap.get is 13.8% of proxy-vue's instructions; the clone and drop alone are 10%. Cost scales linearly with key count (5.7, 12 and 38 µs per lookup at N=250, 1000, 4000).
   - Expected: ~1.15x on proxy-vue, and likely hash-map and lazy-collections (unverified). Probably overlaps the alloc-gc theme; file it once.
2. **`perf(proxy): native descriptor path for [[Set]] with a proxy receiver`**
   - Cause: `property.rs:918` onward materialises JS descriptor objects (`from_property_descriptor` / `to_property_descriptor`) even when the handler has no descriptor traps. ~50% of a proxied write, including plain `p.x = v` with no set trap.
   - Expected: proxy_set 3.4x → ~2.2x, proxy_notrap 3.2x → ~2.2x.
3. **`perf(classes): cheaper construction`**
   - Cause: `eval.rs:234`, `:6661`, `:7029` deep-clone `Vec<InstanceFieldDef>` (initialiser ASTs) per `new`. An init environment is built even for field-less classes and plain functions, and private fields cost a `format!` and a HashMap clone each.
   - Expected: ~5% on every `new`, ~20% for private-field classes.
4. **`perf(classes): id-based private names plus bytecode ops`**
   - Cause: `resolve_private_name` (`eval.rs:142`) walks the env chain and clones Strings on every access, and `compiler.rs:371` and `:458` bail on `#x`. That accounts for 44 of 63 compile bails in raytrace-private.
   - Expected: closes the 28% raytrace-private vs raytrace-public gap (bounded by how little the VM buys on call-dominated code).

## Not reproduced

Class vs function construction, public fields, private slower than public, proxy trap dispatch, getter/setter dispatch beyond generic property cost, `super` calls, and GC (≤3%) all showed no theme-specific gap.
