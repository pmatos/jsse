# #1055: reuse property-key strings across Proxy traps and Reflect

A String key crossing a Proxy trap was converted `JsPropertyKey -> JsString -> JsPropertyKey` with allocations at every hop:
`symbol_key_to_jsvalue` built an intermediate key (for a `&str` name) and a fresh UTF-16 `JsString`, the trap's
`Reflect.get(t, k, r)` re-encoded that string to WTF-8 through `Interpreter::to_property_key`, and both temporaries were
freed after the call.

## Change

- `key_intern.rs`: `KeyStringCache` (the address-keyed `JsPropertyKey -> JsString` cache that `Object.keys`/for-in use) also
  records the reverse direction for strings handed to a Proxy trap (`key_to_trap_js_string`): `(string, key)` goes in a second
  direct-mapped table indexed by the string's first-unit address, and `key_from_js_string` returns the interned key from it,
  falling back to `JsPropertyKey::from_js_string`. `Object.keys`/for-in keep the plain `key_to_js_string` and pay nothing
  extra. A slot holds a clone of the string, so its units cannot be freed or rewritten while it lives, and a hit is confirmed
  by `JsString::ptr_eq` (same units *and* same length, so a longer view over the same buffer misses). A const-initialised
  `Cell<usize>` tag array in front of it makes the lookup that misses (every ToPropertyKey of a string no trap argument
  built) a TLS load, a multiply and a compare, with no `RefCell` or lazy-init check.
- `name_to_js_string` fronts the cache for a name given as text (`get_object_property(.., name: &str)` from `Op::GetProp`, the
  `K = str` instantiation). It is found by the `&str`'s address and length and confirmed by comparing its bytes, so a recycled
  address only misses, and it needs no interner hashing. Hashing through the interner was the dominant remaining cost, and
  an interner at its entry limit hands out a fresh key every time, so a cache keyed by the interned key would never hit for such a name.
- `symbol_key_to_jsvalue` takes the new `PropertyKeyLike::to_trap_key_string` for every non-Symbol key and no longer builds
  an exact key first; the Symbol path (#1054) is untouched (`is_symbol_key`, the 0xFF sigil test).
- `Interpreter::to_property_key` and `to_property_key_string` use `key_from_js_string`; the integer fast path uses
  `JsPropertyKey::from_index` (a shared table below 64, one stack-formatted allocation above) instead of
  `u32::to_string` plus a copy.
- Everything is `thread_local`, holds no object ids and needs no rooting. Retained strings are bounded: at most
  `KEY_STRING_CACHE_SLOTS` (1024) per table, each at most `MAX_CACHEABLE_KEY_BYTES` (256) bytes, so a grown concat buffer is
  not pinned by a long string. No `unsafe` was added.

Not done: step 3 of the issue as a *general* string -> key memo (populating on every `ToPropertyKey` miss). Only strings
built as trap arguments are recorded, so a plain `o[k]` with an unrelated string pays only the tag check.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `95060058` (binary sha256 `f2e43bead9f2...`),
new = this branch at `4373afdf` (`01ed35ea6416...`). Repros are the issue's, plus `strkey-fn.js` (400,000 `o[keys[i]]`
string-variable reads inside a function) as the miss-cost control and `dict-keys.js` (`Object.keys` + for-in over a
6,000-key object, 40 times) as the producer-side control. The empty-script startup is 7.52M Ir.

| repro | base Ir | new Ir | delta |
|---|---:|---:|---:|
| `proxy-trap-named.js` (20,000 x `p.x + p.y`) | 295,518,922 | 273,074,704 | -22.44M: -7.6% gross, -7.8% net of startup |
| `proxy-trap-index.js` | 405,912,650 | 376,724,884 | -29.19M: -7.2% gross |
| `proxy-trap-has-named.js` (`"x" in p`, literal key) | 319,233,366 | 315,191,241 | -1.3% |
| `strkey-fn.js` (control, every lookup misses) | 1,332,016,669 | 1,335,684,854 | +0.28% (9 Ir per string ToPropertyKey) |
| `dict-keys.js` (control) | 768,012,555 | 769,277,973 | +0.16% |

The `in` repro gains little because a string literal is materialised afresh on every evaluation, so the Reflect hop cannot
find it; a string that came from a trap argument does.

Per trap call on the named repro (callgrind, proflto builds, 40,000 traps): `symbol_key_to_jsvalue::<str>` 375 -> 109 Ir,
`to_property_key` from `Reflect.get` 243 -> 101 Ir, and the `Arc<StrView>` (131 Ir) and `Arc<[u8]>` (89 Ir) drops became
refcount decrements.

JetStream drivers at `--iterations 1`. Their Ir varies about 1.5-2% run to run (randomised interner/atom seeds), so
proxy-* are 30 runs each (proxy-vue earlier: 40 runs, median -1.00%, mean -0.56%), the others 3:

| benchmark | base Ir (median) | new Ir (median) | delta median / mean |
|---|---:|---:|---:|
| proxy-vue, 30 runs | 1,395,153,640 | 1,378,417,044 | -1.20% / -0.64% |
| proxy-mobx, 30 runs | 749,022,150 | 749,299,384 | +0.04% / +0.19% |
| fib | 7,425,598,981 | 7,425,605,516 | 0% |
| richards | 13,391,780,652 | 13,397,147,346 | +0.04% |
| delta-blue | 8,777,229,967 | 8,779,345,311 | +0.02% |
| closures / array / string | | | within 0.01% |
| json | 337,570,968 | 338,059,434 | +0.14% |
| object | 1,555,944,216 | 1,558,685,137 | +0.18% |

The +0.1..0.3% on json/object/strkey is the tag check on string ToPropertyKey misses. proxy-mobx exchanges symbol keys
mostly, so it gains nothing and pays that check.

proxy-vue's callgrind view (flat, proflto): the hops the issue costed at 34.7M were already 36M inclusive on main
(`to_property_key` 14.7M, `symbol_key_to_jsvalue::<str>` 11.1M, `<JsPropertyKey>` 1.1M, `Arc<StrView>`/`Arc<[u8]>` drops 9.4M,
not all of them trap-side) and are ~24M now. malloc calls under callgrind fall from 489,381 to
350,830 (-28%, -138.5k), against the issue's 183k round-trip blocks (measured at the previous head `85d0953b`).

## Correctness gates

`test262-extra/Proxy-trap-string-key-identity-round-trip.js` (cites the Proxy `[[Get]]` algorithm and `Reflect.get`) sends
names, the empty string, lone surrogates, non-BMP pairs, `"01"`/`"4294967295"`-style non-indices and index keys on both sides
of the small-index table (0..129) plus a 300-byte key through the get/has/set/deleteProperty traps and the trap's own
`Reflect.*`, checking the trap argument is `===` the original, the right property is addressed, a grown-string
prefix/extension pair do not collide, and that an object key is converted (`toString`) once before the trap sees a String.
Unit tests in `key_intern.rs` cover the reverse cache hit, lone-surrogate/index/empty exactness, same-buffer views of
different length, and a `&str` whose bytes change at the same address.
