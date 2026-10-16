# #1054: memoize symbol property keys and recover the original symbol

`JsSymbol::to_property_key` rebuilt the `Symbol(<desc>)#<id>` key (a WTF-8 encode, an id format and an `Arc<[u8]>` copy)
on every `ToPropertyKey` of a user Symbol, and `symbol_key_to_jsvalue` rebuilt a fresh `JsSymbol` from the key text
(parse plus a linear scan of `global_symbol_registry`) on every key-to-Symbol step (Proxy traps, `Reflect.ownKeys`).

## Change

- `JsSymbolData` gained `key: OnceLock<JsPropertyKey>`. A user symbol builds its key once, so a later conversion is a
  refcount bump. The memo is not interned: the interner is bounded and never evicts, and reads (`obj[sym]`, `sym in obj`)
  must not fill it with dead symbol keys; a property store still interns as before. The key holds bytes only, never the symbol, so there is no `Arc` cycle. Well-known symbols keep the precomputed
  id-less table (#1048). `JsSymbol` stays one pointer (#404).
- The first build registers the symbol in a thread-local `SymbolIndex` (`id -> Weak<JsSymbolData>`). The table never
  keeps a symbol alive; dead entries are swept whenever it doubles (floor 1024), so it is bounded by live symbols plus
  at most the same number of dead ones. `JsSymbol::from_property_key` upgrades the entry and checks that the symbol's
  memoized key is the key asked for (a second interpreter on the thread may reuse an id), so a hit is always the
  original symbol, with identity and description (#1049) intact. A dead symbol (only its key survives on an object)
  falls back to the old reconstruction.
- The cold path (`init_property_key`) is out of line and the hit path is `#[inline]`.
- `vm.rs`: the Number fast paths of the binary-operator and compare-and-jump arms discarded their operands with
  `Vec::truncate`, which calls the slice drop glue. Adding a field to `JsSymbolData` alone (no other change) made LTO
  outline that glue and cost fib/richards/delta-blue ~+0.9% Ir. Both operands were just read as Numbers and own
  nothing, so `discard_number_operands` (one `unsafe` helper, `debug_assert`ed) lowers the length directly. A safe
  `pop()` pair was tried and measured worse (fib +0.7%).
- Not done: step 5 of the issue (`registered` flag for `can_be_held_weakly`).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `814eba00` (binary sha256 `5767cef84b64...`),
new = this branch at `996d467e` (binary sha256 `3bfd9c7f6942...`). Net = minus `empty.js` (7.49M).

The issue's absolute baselines (116.95M, 107.2M, 188.5M) were taken at `2ea6417`; #1048/#1049 and the other perf work
since had already cut the symbol path, so the gain left on main is smaller than the issue's headline.

| repro | base Ir | new Ir | delta (net) |
|---|---:|---:|---:|
| `symkey-read.js` MODE=sym | 54,082,799 | 38,185,152 | -34% |
| `symkey-read.js` MODE=str (control) | 48,902,018 | 48,914,302 | 0% |
| `wk.js` | 38,331,566 | 38,065,265 | -1% |
| `proxy-trap-key.js` MODE=sym | 107,559,541 | 84,873,935 | -23% |

MODE=sym now costs less than the string control. `wk.js` changes little because well-known symbols already used the
precomputed table.

JetStream / micro drivers at `--iterations 1` (proxy-* are medians of 9 and 5 runs; their Ir varies ~+-2% run to run):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| proxy-mobx (median of 9) | 788,323,608 | 783,572,413 | -0.6% |
| proxy-vue (median of 5) | 1,486,774,157 | 1,487,116,722 | 0% |
| fib | 7,530,577,573 | 7,560,205,791 | +0.4% |
| richards | 13,858,801,499 | 13,878,740,227 | +0.1% |
| delta-blue | 9,110,187,042 | 9,140,264,818 | +0.3% |

Callgrind of proxy-mobx on main: `JsSymbol::to_property_key` 8.99M inclusive (1.14%) and `Interpreter::to_property_key`
12.26M (1.56%); with the memo they are 0 and 3.43M (0.44%). The 6.04% the issue measured at `2ea6417` no longer exists
on main, so the issue's proxy-mobx >= 5% criterion cannot be met any more; the whole remaining ceiling was ~1.1%, inside
the noise of the total.

The controls sit +0.1..0.4% above base. fib never touches a Symbol, so this is LTO layout noise, not an executed-path
cost.

## Correctness gates

`test262-extra/Proxy-trap-symbol-key-identity-and-description.js` (cites the Proxy `[[Get]]` algorithm) checks identity
and description of a Symbol key through the get/set/has/delete/defineProperty/getOwnPropertyDescriptor/ownKeys traps for
undefined, empty, lone-surrogate and well-known-looking descriptions, and the description of a key whose Symbol was
dropped (after `$262.gc()`). Unit tests cover memo reuse, the round trip, weak (non-retaining) behaviour, id reuse and
the sweep.
