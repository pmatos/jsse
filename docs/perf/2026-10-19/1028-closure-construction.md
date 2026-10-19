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
| arrow | 1,398 | 1,367 | -2.2% |
| strict function expression | 2,239 | 2,166 | -3.3% |
| sloppy function expression | 2,816 | 2,732 | -3.0% |
| named function expression | 4,509 | 3,734 | -17.2% |
| function declaration hoisted in a block | 5,421 | 4,673 | -13.8% |
| `[i, i + 1]` / 4 elements | 1,250 / 1,524 | 1,229 / 1,504 | -1.7% / -1.3% |
| `{a: i, b: i}` | 1,273 | 1,252 | -1.6% |

Startup (`empty.js`): 5,493,754 -> 5,477,357 Ir.

Whole-process Ir, control set and the rest of the suite (one deterministic run each; `proxy-vue`, `proxy-mobx`, `ed25519`, `js-tokens`
and `ai-astar` move by about 1-2% between two runs of the same binary because the identifier hasher is randomised, so their rows use the
minimum of three runs of each binary):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| closure_create | 2,466,480,716 | 2,441,372,575 | -1.02% |
| promise_new | 5,790,266,933 | 5,747,526,143 | -0.74% |
| crypto | 3,872,674,829 | 3,849,310,228 | -0.60% |
| earley-boyer | 4,958,811,066 | 4,933,695,955 | -0.51% |
| bigint-noble-bls12-381 | 55,409,565,929 | 55,209,412,955 | -0.36% |
| js-tokens (min of 3) | 4,718,744,929 | 4,642,790,656 | -1.61% |
| closures | 5,730,598,231 | 5,717,851,688 | -0.22% |
| Box2D | 5,352,800,449 | 5,345,861,540 | -0.13% |
| raytrace | 11,496,077,259 | 11,478,663,075 | -0.15% |
| splay | 7,576,043,466 | 7,574,902,890 | -0.02% |
| delta-blue | 6,211,567,264 | 6,210,951,620 | -0.01% |
| richards | 8,444,258,600 | 8,444,231,039 | -0.00% |
| fib | 4,896,578,549 | 4,896,561,937 | -0.00% |
| opmix | 38,800,958,412 | 38,800,942,873 | -0.00% |
| array | 721,822,364 | 721,966,599 | +0.02% |
| gaussian-blur | 19,901,439,531 | 19,914,746,438 | +0.07% |
| lazy-collections | 3,720,007,498 | 3,710,967,627 | -0.24% |
| bigint-noble-secp256k1 | 4,777,197,659 | 4,764,222,255 | -0.27% |
| ai-astar (min of 3) | 9,974,083,176 | 9,980,502,789 | +0.06% |
| bigint-noble-ed25519 (min of 3) | 5,522,194,187 | 5,538,663,303 | +0.30% |
| proxy-mobx (min of 3) | 549,263,445 | 547,317,004 | -0.35% |
| navier-stokes (alone) | 3,236,084,780 | 3,156,704,576 | -2.45% (layout; its Ir moves by +-2.5% with any rebuild) |

The remaining 18 workloads (Air, Babylon, Basic, ML, async-fs, bigint-bigdenary, doxbee-*, hash-map, json, json-parse-inspector, loop,
object, regex, stanford-crypto-sha256, string, sync-fs, proxy-vue) are within +-0.2% of base.

Peak RSS (`/usr/bin/time -v`, three runs each): `closure_create` 25.8 -> 23.5 MB, `closures` 25.8 -> 23.4 MB; `array`, `Box2D`, `js-tokens`,
`splay`, `richards` unchanged within +-1%.

Wall time is indicative only (load average 2-3 on the shared host, nine interleaved runs pinned to one CPU, median): `closure_create`
-0.5%, `closures` +0.2%, `js-tokens` -1.4%, `richards` +0.7%, `delta-blue` +0.3%, `array` +2.2%, `splay` +1.5%, `Box2D` +2.1%,
`lazy-collections` +1.6%, against identical-Ir hot loops. Rebuilding with any unrelated change shifts these rows by up to +-4% in either
direction (builds that differ only in unrelated code moved `richards` between -0.8% and +4.4% at identical instruction counts, and a copy
of the base binary reads +0.6% against itself), so a wall difference at equal Ir is code layout, not work.

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
