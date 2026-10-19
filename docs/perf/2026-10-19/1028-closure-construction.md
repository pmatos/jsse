# #1028: closure construction

Re-profiled on `a217156d` (callgrind, shipped inlining plus line tables, `closure_create` and per-shape micros of 50k and 150k
iterations; Ir per iteration is the difference, minus the loop control of 610 Ir).

| shape (one evaluation per iteration) | Ir / iteration |
|---|---:|
| arrow | 1,398 |
| strict function expression (`length`, `name`, `prototype`) | 2,239 |
| sloppy function expression (+ `caller`, `arguments`, a heap-backed property map) | 2,816 |
| named function expression | 4,509 |
| function declaration hoisted in a block | 5,421 |
| `[i, i + 1]` | 1,250 |
| `{a: i, b: i}` | 1,273 |

Where a sloppy closure's ~2,800 Ir go (150k closures, callgrind): `create_function_with_name` 1,700 (`PropertyMap::reserve` 330, six
`append`s 394, two `alloc_object` 317 + 128 of `memcpy`, the `Box<JsFunction>` malloc 103), the sweep and drop of the two objects ~1,000
(`PropertyMapInner` drop 217 per object, `free_gc_object` 235, the `Box` free 123), the rest dispatch and the call. A named function
expression additionally interned its name (`BindingMap::insert` 520), built its own `Environment` (316), looked the name up again to
initialize it (`env_set` 141) and allocated the name `JsString` and a `String`; a hoisted declaration interned its name twice.

## Change

The new code lives in `function_objects.rs` (the moved `make_closure`, `create_function_with_name` and `hoist_function_decl`),
`types/function_props.rs` and `property_map/exact_capacity.rs`, so no file over the token limit grows.

- **Own-name binding without hashing.** A named function expression's scope gets its binding with `push_unique` of the literal's cached
  atom (`LiteralFrame::own_name_atom`) and initializes entry 0 in place, instead of `declare` + `env_set` by text. `hoist_function_decl`
  inserts a local scope's binding by atom in one step; a scope that mirrors onto the global object or holds module imports keeps the
  declare-then-assign path.
- **`name` string per literal.** `LiteralFrame::name_string` holds one `JsString` per literal (re-checked against the name, because the
  default class constructors share one frame across names) instead of allocating one per closure (the `String` in `JsFunction::User.name` is still allocated per named closure).
- **Exact property storage for functions.** `PropertyMap::with_exact_capacity` + `push_non_index` build `length`, `name`, `caller`,
  `arguments`, `prototype` into one right-sized buffer with no grow path, duplicate scan or digit check. A sloppy closure's buffer is
  240 bytes instead of the next-larger growth step (peak RSS of `closure_create`/`closures` -8%).
- **Async-generator queue.** `free_gc_object` skips the `async_gen_queues.remove` hash probe when the table is empty (it was a hash per
  freed object).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`; base = `a217156d`)

Ir per iteration net of the loop control (callgrind/cachegrind whole-process `I refs` differences of 50k and 150k iterations):

| shape | base | new | delta |
|---|---:|---:|---:|
| arrow | 1,398 | 1,363 | -2.5% |
| strict function expression | 2,239 | 2,161 | -3.5% |
| sloppy function expression | 2,816 | 2,735 | -2.9% |
| named function expression | 4,509 | 3,728 | -17.3% |
| function declaration hoisted in a block | 5,421 | 4,693 | -13.4% |
| `[i, i + 1]` | 1,250 | 1,229 | -1.7% |
| `{a: i, b: i}` | 1,273 | 1,252 | -1.6% |

Whole-process Ir, one deterministic run each unless marked. `proxy-vue`, `js-tokens` and a few other benchmarks move by about 1-2%
between two runs of the same binary because the identifier hasher is randomised, so those rows (marked) use the minimum of three runs of
each binary:

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| promise_new | 5,790,266,933 | 5,721,706,147 | -1.18% |
| closure_create | 2,466,480,716 | 2,440,953,188 | -1.03% |
| closures | 5,730,598,231 | 5,686,925,303 | -0.76% |
| earley-boyer | 4,958,811,066 | 4,928,351,004 | -0.61% |
| crypto | 3,872,674,829 | 3,850,583,709 | -0.57% |
| hash-map | 12,876,469,467 | 12,823,519,886 | -0.41% |
| lazy-collections | 3,720,007,498 | 3,711,064,219 | -0.24% |
| raytrace | 11,496,077,259 | 11,471,476,140 | -0.21% |
| delta-blue | 6,211,567,264 | 6,208,597,909 | -0.05% |
| Box2D | 5,352,800,449 | 5,350,824,127 | -0.04% |
| splay | 7,576,043,466 | 7,574,149,287 | -0.03% |
| ai-astar | 9,976,681,580 | 9,973,886,514 | -0.03% |
| richards | 8,444,258,600 | 8,444,226,990 | -0.00% |
| fib | 4,896,578,549 | 4,896,555,784 | -0.00% |
| opmix | 38,800,958,412 | 38,800,939,387 | -0.00% |
| gaussian-blur | 19,901,439,531 | 19,901,425,793 | -0.00% |
| array | 721,822,364 | 722,098,491 | +0.04% |
| bigint-noble-secp256k1 | 4,777,197,659 | 4,778,826,234 | +0.03% |
| js-tokens (min of 3) | 4,640,127,214 | 4,617,150,345 | -0.50% |
| proxy-vue (min of 3) | 1,084,349,639 | 1,077,407,662 | -0.64% |
| regex (min of 3) | 116,742,136 | 116,763,347 | +0.02% |
| navier-stokes (min of 3) | 3,152,991,757 | 3,153,088,674 | +0.00% |

The other workloads (Air, Babylon, Basic, ML, async-fs, bigint-bigdenary, bigint-noble-*, doxbee-*, json, json-parse-inspector, loop,
object, proxy-mobx, stanford-crypto-sha256, string, sync-fs) are within +-0.3% of base. Startup (`empty.js`): 5,493,754 -> 5,477,357 Ir.

Peak RSS (`/usr/bin/time -v`, three runs each): `closure_create` 25.7 -> 23.7 MB, `closures` 25.9 -> 23.6 MB (-8%); `Box2D` and `richards`
unchanged within +-1%.

Wall time (load average 1.6-2.4, nine interleaved runs pinned to one CPU, median; indicative, above the 1.5 gate): `closure_create` -2.7%,
`closures` -1.1%, `Box2D` -1.3%, `delta-blue` -0.4%, `splay` -0.2%, `richards` 0.0%, `lazy-collections` +0.1%, `array` +0.6%, `js-tokens`
+0.9%. Wall at equal Ir is code layout: builds that differ only in unrelated code moved `richards` between -0.8% and +4.4% at identical
instruction counts, and a copy of the base binary reads +0.6% against itself.

## Tried and dropped

- **Shared immutable property prefix.** One `Rc` holding a closure's `length`, `name`, `caller` and `arguments` records, with a copy on the
  first write, removed the records, the spill and their drop entirely: sloppy closure -803 Ir (-23%), arrow -247, `closures` -10% wall. Every way of making `PropertyMap` aware of it (a third `PropertyMapInner` variant, or a `prefix` field consulted when a lookup
  misses) cost 2-4% Ir on the property-heavy controls (`ai-astar` +4.3%, `array` +4.4%, `raytrace` +3.9%, `richards` +1.9%, `getproto`
  micro +169 Ir per iteration): `slot_hinted`'s inlined copy of `slot_of` grew past the inliner's threshold and became an out-of-line call
  (`slot_of::<JsPropertyKey>` 69.6M Ir on a 150k loop), and even with `#[inline(always)]` the hit path carried +11 spills and moves. A
  variant that added the field but left every lookup unchanged was Ir-identical to base, so layout is not the cause. Copying a template
  into a fresh map (no sharing) was no cheaper than the six appends.
- **`alloc_object` expanded at the hot allocators** (closure, array, object literal, prototype) removes the `memcpy` of the 312-byte
  payload into an argument slot (-128 Ir per object, `js-tokens` -8% Ir) but did not move wall time (the copy is cheap in cycles) and
  bloated instruction-cache misses on the closure benchmarks, so it is not shipped.
- **Lazy `.prototype` object** (about 850 Ir per constructable closure: allocation, `constructor` push, sweep). A placeholder record needs
  every reader of a function's own properties to materialize it with `&mut Interpreter`, but the property read entry points
  (`get_property_on_id`, `has_property_on_id`, ...) take `&self`; the alternative, reserving the arena id and allocating on first
  dereference, needs interior-mutable arena allocation and GC rules for an id whose value may already have escaped. Neither fits without
  touching the own-lookup miss path that the item above shows is codegen-sensitive.
