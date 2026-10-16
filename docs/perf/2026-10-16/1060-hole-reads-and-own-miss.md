# #1060: dense hole reads and single-scan own-property misses

## Change

**A. Hole and out-of-range reads of an Array** (`numeric_index_fast_get`, `property.rs`). The Array branch used to give up on
a backing-vector `undefined` and on `index >= len`, so `Op::GetElement` formatted the index (`to_property_key`: two mallocs)
and walked the own list, `Array.prototype` and `Object.prototype` with string keys. It now returns `undefined` directly for
any integer index in `0..2^32-1` that is absent from the vector (hole or past the end) when
- the receiver owns no digit-leading key (`PropertyMap::may_hold_index_key`, so no descriptor can be a present `undefined`
  or an accessor at that index), and
- the prototype chain is made only of ordinary non-String-wrapper objects and empty Arrays that own no digit-leading key
  (`array_dense::proto_chain_without_index_keys`). This is the read-side twin of `proto_chain_index_free` (they share
  `walk_index_free_chain`): a link that owns any digit-leading key declines instead of scanning its key list, so a program
  that sets `Array.prototype[0]` pays the generic path, not a key scan per hole read. Proxies, TypedArrays, String wrappers
  and any other kind on the chain decline.

Anything else falls through to the unchanged generic path, so inherited indices, index accessors and Proxy `get` traps still
run. The helper also serves the tree-walker `access.rs`, `Array.prototype.join` and the array iterator.

**B. Own-property miss scans the list once** (`get_object_property_with_proxy_depth`). After `properties.get(key)` missed it
called `get_own_property_full`, whose first statement is the same `properties.get(key)`. `get_own_property_full` is now that
lookup plus the new `JsObjectData::get_own_exotic_property` (Array element, TypedArray, String exotic); the [[Get]] fast path
calls the latter directly. A mapped `arguments` object keeps the full call (its parameter map is consulted with `properties`).
The String-exotic test checks `primitive_value` before `class_name`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`, sequential. base = `origin/main` at `02d92938`
(sha256 `97f59a65624d...`), new = base plus this change (`e7cbea797c10...`).

Issue reproducer, Ir per iteration = (N=250,000 minus N=50,000) / 200,000:

| case | base | new | delta | criterion |
|---|---:|---:|---:|---|
| A hole read of `new Array(1024)` (PART 0 MODE 0) | 3,098 | 1,951 | -37.0% | >= 35% |
| A read past the vector (MODE 2) | 3,106 | 1,887 | -39.2% | >= 35% |
| A present element (control, MODE 1) | 1,684 | 1,691 | +0.4% | within 1% |
| B 4 own-hit reads (control, PART 1 MODE 0) | 3,559 | 3,526 | -0.9% | within 1% |
| B 2 own misses then a hit (MODE 1) | 6,209 | 5,101 | -17.8% | >= 12% |
| B 3 own misses then a hit (MODE 2) | 7,378 | 5,923 | -19.7% | >= 12% |
| B absent key, 5 misses (MODE 3) | 8,744 | 6,472 | -26.0% | >= 12% |
| `new Array(n).join("ab")`, per element | 1,146 | 253 | -77.9% | |

The present-element control showed one 12M-Ir outlier run on the new binary (a known whole-run flip); three repeats give
432.47M vs 430.98M for base.

JetStream drivers at `--iterations 1`, whole-process Ir:

| benchmark | base | new | delta | criterion |
|---|---:|---:|---:|---|
| hash-map | 19,211,496,392 | 18,495,904,070 | -3.73% | >= 2.5% |
| delta-blue | 9,001,605,921 | 8,782,550,400 | -2.43% | >= 1.8% |
| richards | 13,599,946,308 | 13,391,395,258 | -1.53% | >= 1% |
| Babylon | 3,375,829,908 | 3,303,893,137 | -2.13% | |
| Box2D | 8,228,037,663 | 8,129,753,325 | -1.20% | |
| raytrace | 17,156,291,097 | 17,034,843,957 | -0.71% | |
| splay | 10,433,510,486 | 10,399,944,572 | -0.32% | |
| fib | 7,425,604,624 | 7,425,598,384 | 0.00% | no regression |
| gaussian-blur | 31,302,482,823 | 31,300,020,929 | -0.01% | no regression |
| `bench_array.js` | 947,542,649 | 944,344,945 | -0.34% | no regression |
| `bench_opmix.js` | 61,535,146,995 | 61,122,234,233 | -0.67% | no regression |
| `bench_object.js` | 1,556,812,487 | 1,555,775,107 | -0.07% | no regression |

Callgrind (shipped profile plus line tables, frame pointers): `get_own_property_full` no longer appears anywhere in the
delta-blue profile (base: `::<str>` 83.3M self, 0.89%, plus its inlined slice/property_map lines). hash-map calls
`to_property_key` 352,146 times on base (704,292 allocator calls) and 5 times now. dhat cannot see the
shipped allocator (jemalloc), so the allocation check is the callgrind call count.

## Correctness gates

`test262-extra/array-hole-and-own-miss-reads.js` covers: holes and out-of-range reads with no inherited index; inherited
`Array.prototype[i]`/`Object.prototype[i]` (added after earlier reads, shadowing, deletion); index accessors with the
array as receiver and accessors without a getter; `4294967294` vs `4294967295`, `-1`, `1.5`, `NaN`, `-0`; present
`undefined` (literal, assigned, `fill(undefined)`) shadowing an inherited index versus a hole, `delete` and `length`
truncation; an own index accessor beside a hole and a frozen array; a Proxy as the array's prototype and behind
`Array.prototype` (trap runs for hole and out-of-range reads, not for present elements); Array subclasses and replaced
prototypes; a String wrapper as prototype and as receiver; TypedArray out-of-range with `Object.prototype[i]`; mapped and
unmapped `arguments`; own-miss chains with data, accessor and getter-less holders; `join`, spread and `Array.from` over
holes. It passes on the base binary too (no semantic change), in both modes, under `JSSE_GC_STRESS=7`, and under node.
