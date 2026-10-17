# #1029: named property get, store, method load and update complete in place on the VM

Re-profiled `origin/main` `20984682` (callgrind, line tables, Ir) on richards and Box2D. The named store was the largest
per-operation deficit: `SetProp` went `member_set` -> `put_member_value` -> `set_object_with_key_result` ->
`put_value_to_property` -> `set_object_property_with_outcome` -> `proxy_set_with_outcome_and_proxy_depth`, which builds a
`PropertyDescriptor` (`get_own_property`: clones value/getter/setter), scans the property list a second time in
`set_property_value`, then writes the barrier. Box2D: `member_set` 791M inclusive (11.2% of Ir), `put_member_value` 762M.
Gets cost less per op, but `GetProp`/`LoadMethod` paid the same chain (`member_get` 1.72G = 24.4%, `get_object_property_with_proxy_depth`
1.49G = 21.1% on Box2D) and `PropertyMap::slot_of` compared every key by `memcmp` (richards: 26M calls, 3.3% of Ir).

## Change

- **`SetProp`** (`Interpreter::named_store_fast`): an ordinary receiver (not Proxy, module namespace, TypedArray or arguments
  object) with an own, writable data property takes the value in place, followed by the write barrier
  (`gc_write_barrier_value`, the named store has no element-card barrier; nothing can collect in between). An absent property is added when the receiver is
  extensible, is not an Array or a String wrapper, and every object up the prototype chain is ordinary and does not own the name
  (no setter, read-only property, Proxy or exotic lookup can intervene). Array `length`, Array-index-like names, accessors,
  non-writable properties, frozen/sealed receivers and primitives take the generic [[Set]] unchanged.
- **`GetProp` / `LoadMethod`** (`Interpreter::named_get_fast`): walks the chain of ordinary objects to the holder of the name
  and returns its data value, or its getter (called with the original receiver, with no second walk); a Proxy,
  TypedArray, module namespace, String wrapper or primitive anywhere on the way falls back to the generic path.
- **`UpdateProp`** (`obj.name++`/`--`): an own writable Number data property is updated in place.
- **Key comparison** (`key_bytes_eq`): interned keys of equal text share one allocation, so the property-list scan settles a
  hit by address and rejects most mismatches on length or first byte before any `memcmp`.
- The four ops are out-of-line functions (`get_prop_op`, `load_method_op`, `set_prop_op`, `update_prop_op`: fast path and
  generic path together), so the dispatch loop carries one call per op. A first version inlined the fast paths into
  `run_chunk_inner` and regressed workloads that never touch a named property (navier-stokes +3.9%, gaussian-blur +3.1%,
  opmix +2.3%) through code generation in the dispatch loop alone; moving the bodies out removed that.
  A second regression (sync-fs +2.8%) came from repeating the chain walk when the property turned out to be an accessor;
  returning the getter from the first walk removed it.

No `unsafe`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` `20984682` (binary sha256 `38009b7f297fe4c4...`),
new = base plus this change (JetStream drivers at `--iterations 1`; micro suite as shipped).

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| raytrace | 14,900,173,669 | 12,181,432,693 | -18.25% |
| richards | 11,256,964,264 | 9,433,317,701 | -16.20% |
| ai-astar | 12,547,635,071 | 10,658,135,705 | -15.06% |
| Babylon | 2,869,547,310 | 2,568,638,304 | -10.49% |
| Box2D | 7,051,965,783 | 6,353,602,366 | -9.90% |
| js-tokens | 5,082,965,440 | 4,671,823,409 | -8.09% |
| earley-boyer | 5,575,751,350 | 5,170,043,318 | -7.28% |
| splay | 8,772,451,318 | 8,159,141,242 | -6.99% |
| hash-map | 15,162,198,693 | 14,155,952,158 | -6.64% |
| delta-blue | 6,973,834,882 | 6,513,802,328 | -6.60% |
| bigint-noble-bls12-381 | 63,594,146,771 | 60,363,723,330 | -5.08% |
| bigint-bigdenary | 4,762,871,034 | 4,528,889,443 | -4.91% |
| proxy-mobx | 628,457,131 | 601,275,828 | -4.33% |
| proxy-vue | 1,173,319,856 | 1,154,835,205 | -1.58% |
| bigint-noble-ed25519 | 7,253,406,428 | 7,162,686,225 | -1.25% |
| bigint-noble-secp256k1 | 6,360,475,845 | 6,296,433,714 | -1.01% |
| Air | 1,937,906,901 | 1,924,463,737 | -0.69% |
| ML | 56,428,467,303 | 56,094,264,204 | -0.59% |
| lazy-collections | 4,410,465,589 | 4,398,546,324 | -0.27% |
| crypto | 4,933,720,159 | 4,924,965,078 | -0.18% |
| micro:closures | 5,851,883,476 | 5,841,650,445 | -0.17% |
| micro:fib | 5,650,476,320 | 5,643,748,055 | -0.12% |
| micro:string | 342,280,515 | 342,186,017 | -0.03% |
| micro:loop | 3,184,543,023 | 3,185,547,868 | +0.03% |
| json-parse-inspector | 9,161,548,759 | 9,168,107,706 | +0.07% |
| stanford-crypto-sha256 | 4,530,408,434 | 4,535,221,636 | +0.11% |
| sync-fs | 6,771,457,028 | 6,778,983,157 | +0.11% |
| micro:promise_new | 5,878,439,041 | 5,890,868,074 | +0.21% |
| doxbee-promise | 4,002,525,096 | 4,013,376,193 | +0.27% |
| micro:closure_create | 2,626,052,879 | 2,633,972,609 | +0.30% |
| navier-stokes | 3,843,171,951 | 3,854,876,405 | +0.30% |
| async-fs | 7,596,352,010 | 7,622,192,754 | +0.34% |
| micro:json | 314,128,566 | 315,387,736 | +0.40% |
| micro:regex | 117,583,593 | 118,124,897 | +0.46% |
| micro:opmix | 53,232,607,959 | 53,504,687,659 | +0.51% |
| micro:array | 762,942,902 | 767,911,093 | +0.65% |
| gaussian-blur | 26,976,946,538 | 27,189,435,262 | +0.79% |
| micro:object | 1,465,830,209 | 1,477,544,205 | +0.80% |
| Basic | 3,547,493,162 | 3,578,670,779 | +0.88% |
| doxbee-async | 2,344,555,149 | 2,369,170,231 | +1.05% |

Geometric mean over 40 workloads: -3.10%

Per-access micro (function-local object, 8 accesses per iteration, minus a control loop of the same shape; Ir per access):

| access | base | new | delta |
|---|---:|---:|---:|
| `o.a = i` own writable data property (4 properties) | 1,041 | 557 | -46% |
| `o.v3 = i` (6-property constructor-built object) | 1,209 | 667 | -45% |
| `o.n++` own Number property (4 per iteration) | 1,052 | 392 | -63% |
| `o.a` own data property | 474 | 437 | -8% |
| `o.a` data property on the prototype | 589 | 542 | -8% |

Eight `this.n = k` stores in a method called once per iteration save 457 Ir per store (9,682 -> 6,023 per iteration, call included).

Box2D, callgrind inclusive: `member_set` 791M -> `set_prop_op` 392M, `put_member_value` 762M -> 126M (-83%; it now only serves the
cases above that decline).

## Where the rest is

`PropertyMap::get` is now the largest property cost on richards (974M, 10.3% of Ir, ~82 Ir per lookup) and Box2D (782M, 12.5%):
a linear scan of the entries (up to 8, interned keys) per access. A remembered slot per site, verified by key identity (the
mechanism `GlobalHint` already uses for global names), avoids it for own hits; an own miss followed by a prototype hit (method
loads) needs the miss proven without a scan.
