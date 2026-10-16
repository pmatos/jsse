# #1055: reuse property-key strings across Proxy traps and Reflect

A String key crossing a Proxy trap was converted `JsPropertyKey -> JsString -> JsPropertyKey` with allocations at every hop:
`symbol_key_to_jsvalue` built an intermediate key (for a `&str` name) and a fresh UTF-16 `JsString`, the trap's
`Reflect.get(t, k, r)` re-encoded that string to WTF-8 through `Interpreter::to_property_key`, and both temporaries were
freed after the call.

## Change

- `key_intern.rs`: `KeyStringCache` (the address-keyed `JsPropertyKey -> JsString` cache that `Object.keys`/for-in use) also
  records the reverse direction. Whenever it builds a string it files `(string, key)` in a second direct-mapped table indexed
  by the string's first-unit address; `key_from_js_string` looks a String up there and returns the interned key, falling
  back to `JsPropertyKey::from_js_string`. A slot holds a clone of the string, so its units cannot be freed or rewritten
  while the entry lives, and a hit is confirmed by `JsString::ptr_eq` (same units *and* same length, so a longer view over
  the same buffer misses). A const-initialised `Cell<usize>` tag array in front of it makes the lookup that misses (every
  ToPropertyKey of a string no key built) a TLS load, a multiply and a compare, with no `RefCell` or lazy-init check.
- `name_to_js_string` fronts the cache for a name given as text (`get_object_property(.., name: &str)` from `Op::GetProp`, the
  `K = str` instantiation). It is found by the `&str`'s address and length and confirmed by comparing its bytes, so a recycled
  address only misses, and it needs no interner hashing. Hashing through the interner was the dominant remaining cost, and
  an interner at its entry limit hands out a fresh key every time (proxy-vue is in that state), so the cache would never hit.
- `symbol_key_to_jsvalue` takes the new `PropertyKeyLike::to_key_js_string` for every non-Symbol key and no longer builds
  an exact key first; the Symbol path (#1054) is untouched (`is_symbol_key`, the 0xFF sigil test).
- `Interpreter::to_property_key` and `to_property_key_string` use `key_from_js_string`; the integer fast path uses
  `JsPropertyKey::from_index` (a shared table below 64, one stack-formatted allocation above) instead of
  `u32::to_string` plus a copy.
- Everything is `thread_local`, holds no object ids and needs no rooting. Retained strings are bounded: at most
  `KEY_STRING_CACHE_SLOTS` (1024) per table, each at most `MAX_CACHEABLE_KEY_BYTES` (256) bytes, so a grown concat buffer is
  not pinned by a long string. No `unsafe` was added.

Not done: step 3 of the issue as a *general* string -> key memo (populating on every `ToPropertyKey` miss). Only strings
built by the key-string cache are recorded, so a plain `o[k]` with an unrelated string pays only the tag check.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `95060058` (binary sha256 `f2e43bead9f2...`),
new = this branch at `85d0953b` (`222d0662b7d0...`). Repros are the issue's, plus `strkey-fn.js` (400,000 `o[keys[i]]`
string-variable reads inside a function) as the miss-cost control. The empty-script startup is 7.52M Ir.

| repro | base Ir | new Ir | delta (net of startup) |
|---|---:|---:|---:|
| `proxy-trap-named.js` (20,000 x `p.x + p.y`) | 295,519,972 | 273,036,103 | -22.48M, -7.8% (-7.6% gross) |
| `proxy-trap-index.js` | 405,914,382 | 377,308,926 | -28.61M, -7.2% |
| `proxy-trap-has-named.js` (`"x" in p`, literal key) | 319,231,867 | 314,552,097 | -1.5% |
| `strkey-fn.js` (control, every lookup misses) | 1,332,015,792 | 1,335,283,069 | +0.25% (8 Ir per string ToPropertyKey) |

The `in` repro gains little because a string literal is materialised afresh on every evaluation, so the Reflect hop cannot
find it; a string that came from a trap, `Object.keys` or for-in does.

Per trap call on the named repro (callgrind, proflto builds, 40,000 traps): `symbol_key_to_jsvalue::<str>` 375 -> 109 Ir,
`to_property_key` from `Reflect.get` 243 -> 101 Ir, and the `Arc<StrView>` (131 Ir) and `Arc<[u8]>` (89 Ir) drops became
refcount decrements.

JetStream drivers at `--iterations 1`. Their Ir varies about 1.5-2% run to run (randomised interner/atom seeds), so proxy-*
are 15 runs each (and proxy-vue 40 more), others 3:

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| proxy-vue, 40 runs (median / mean) | 1,391,939,799 / 1,395,172,637 | 1,377,998,345 / 1,387,293,392 | -1.00% / -0.56% |
| proxy-vue, 15 runs (median / min) | 1,392,684,665 / | 1,389,411,105 / | -0.24% / -0.84% |
| proxy-mobx, 15 runs | 749,839,994 | 748,724,595 | -0.15% |
| fib | 7,425,598,503 | 7,425,605,186 | 0% |
| richards | 13,390,401,592 | 13,390,625,674 | 0% |
| delta-blue | 8,777,630,284 | 8,776,932,495 | 0% |
| closures / array / json / string | | | 0% |
| object | 1,555,925,454 | 1,557,420,079 | +0.10% |

proxy-vue's callgrind view (flat, proflto): the hops the issue costed at 34.7M were already 36M inclusive on main
(`to_property_key` 14.7M of which the Reflect closures ~6.5M, `symbol_key_to_jsvalue::<str>` 11.1M, the string/key drops
~9M) and are ~24M now, which is the 0.6-1.0% of the run measured above. malloc calls under callgrind fall from 489,381 to
350,830 (-28%, -138.5k), against the issue's 183k round-trip blocks.

## Correctness gates

`test262-extra/Proxy-trap-string-key-identity-round-trip.js` (cites the Proxy `[[Get]]` algorithm and `Reflect.get`) sends
names, the empty string, lone surrogates, non-BMP pairs, `"01"`/`"4294967295"`-style non-indices and index keys on both sides
of the small-index table (0..129) plus a 300-byte key through the get/has/set/deleteProperty traps and the trap's own
`Reflect.*`, checking the trap argument is `===` the original, the right property is addressed, a grown-string
prefix/extension pair do not collide, and that an object key is converted (`toString`) once before the trap sees a String.
Unit tests in `key_intern.rs` cover the reverse cache hit, lone-surrogate/index/empty exactness, same-buffer views of
different length, and a `&str` whose bytes change at the same address.
