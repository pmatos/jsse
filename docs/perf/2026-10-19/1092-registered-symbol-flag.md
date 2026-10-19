# #1092: `registered` flag instead of a registry scan in `can_be_held_weakly`

`Interpreter::can_be_held_weakly` answered "is this Symbol in the GlobalSymbolRegistry?" (§9.13 CanBeHeldWeakly, §20.4.2.6
KeyForSymbol) by scanning every `Symbol.for` entry for each Symbol key handed to a WeakMap/WeakSet/WeakRef/FinalizationRegistry
operation: O(registered symbols) per call.

## Change

- `JsSymbolData` gained `registered: bool`, set only by the new `JsSymbol::for_key` that `Symbol.for` uses (measured under the name `new_registered`; renamed afterwards to keep `primitive_constructors.rs` from growing). The
  flag sits in existing padding: `size_of::<JsSymbolData>()` is 48 before and after (checked with a throwaway unit test), so
  `JsSymbol` stays one word and the symbol allocation is unchanged.
- `can_be_held_weakly` tests `JsSymbol::is_registered`. A symbol rebuilt from a dead property key is never a registered one
  (the registry keeps `Symbol.for` symbols alive), and `from_property_key` returns the original data, so the flag survives
  the key round trip (unit test).
- Token ratchet: `src/types.rs` was 93,646 bytes (limit 20,000, may not grow), so the symbol code (`JsSymbolData`,
  `SymbolIndex`, `JsSymbol` and its tests) moved verbatim to `src/types/symbol.rs`; `types.rs` is now 83,667 bytes.
- Not done: `Symbol.keyFor` and `symbol_key_to_jsvalue` still scan the registry (outside the issue).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

base = `origin/main` `f35eda7d` (binary sha256 `171485ecad4e...`), new = base plus this change (`c061ba2f4849...`).
cachegrind `--cache-sim=no`, whole-process `I refs`, JetStream drivers at `--iterations 1`, two runs of each binary
(minimum shown; the identifier hasher is randomised, so the heavier benchmarks move up to ~0.3% between two runs of one binary,
and one `opmix` run of the new binary that read +1.7% reproduced at 38,800,941,167 vs 38,800,941,436 on rerun).

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| weak-sym-reg1000 (200k `Symbol()` WeakMap/WeakSet set+has, 1000 `Symbol.for` entries) | 16,609,596,473 | 3,012,833,976 | -81.9% |
| weak-sym-reg0 (same, empty registry) | 3,015,585,945 | 3,006,370,338 | -0.3% |
| fib | 4,896,558,397 | 4,896,559,174 | 0.00% |
| richards | 8,444,237,240 | 8,444,260,921 | 0.00% |
| delta-blue | 6,208,734,357 | 6,209,181,666 | +0.01% |
| Box2D | 5,349,071,964 | 5,346,967,572 | -0.04% |
| lazy-collections | 3,711,009,069 | 3,710,973,665 | 0.00% |
| bigint-noble-secp256k1 | 4,758,776,473 | 4,751,816,719 | -0.15% |
| opmix | 38,800,940,487 | 38,800,940,697 | 0.00% |
| gaussian-blur | 19,901,426,192 | 19,901,426,059 | 0.00% |
| splay | 7,572,984,743 | 7,572,937,694 | 0.00% |

Peak RSS (`/usr/bin/time -f %M`): richards 19,388 -> 19,048 KB, splay 566,276 -> 566,428 KB, weak-sym-reg1000 58,488 -> 58,992 KB;
Box2D medians of 10 interleaved runs 38,620 vs 38,624 KB. Binary 31,772,928 -> 31,771,312 bytes.

Wall (hyperfine, 7 runs, load ~2.4 on the shared host, so only the large effect is meaningful): richards 1.943 s vs 1.944 s;
weak-sym-reg1000 2.762 s -> 0.654 s (4.2x).

## Correctness gates

`test262-extra/CanBeHeldWeakly-registered-and-unregistered-symbols.js` (cites CanBeHeldWeakly and KeyForSymbol) checks that
`Symbol.for` symbols (including the empty and `undefined` keys) are rejected and `Symbol()`, same-description and well-known symbols
are accepted by WeakMap, WeakSet, WeakRef, FinalizationRegistry (target, token, unregister) and the WeakMap/WeakSet constructor
iterables. Full test262 (default and `--no-bytecode`): 99,911/99,911, no regressions; test262-extra (both modes): 1,512/1,512; custom 19/19.
