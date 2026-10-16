# #1054: memoize symbol property keys and recover the original symbol

`JsSymbol::to_property_key` rebuilt the `Symbol(<desc>)#<id>` key (a WTF-8 encode, an id format and an `Arc<[u8]>` copy)
on every `ToPropertyKey` of a user Symbol, and `symbol_key_to_jsvalue` rebuilt a fresh `JsSymbol` from the key text
(parse plus a linear scan of `global_symbol_registry`) on every key-to-Symbol step (Proxy traps, `Reflect.ownKeys`).

## Change

- `JsSymbolData` gained `key: OnceLock<JsPropertyKey>`. A user symbol builds its key once, through `intern_js_key`, so a
  later conversion is a refcount bump and a later property store on that key hits the interner's canonical-address path.
  The key holds bytes only, never the symbol, so there is no `Arc` cycle. Well-known symbols keep the precomputed
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
  outline that glue and cost fib/richards/delta-blue ~+0.9% Ir. Both operands are Numbers there and own nothing, so the
  arms now lower the length directly; this removes the sensitivity and is slightly cheaper than before.
- Not done: step 5 of the issue (`registered` flag for `can_be_held_weakly`).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `814eba00` (binary sha256 `5767cef84b64...`),
new = this branch (`ef6c36d3` + follow-ups, see PR). Net = minus `empty.js` (7.49M).

The issue's absolute baselines (116.95M, 107.2M, 188.5M) were taken at `2ea6417`; #1048/#1049 and the other perf work
since had already cut the symbol path, so the gain left on main is smaller than the issue's headline.

| repro | base Ir | new Ir | delta (net) |
|---|---:|---:|---:|
| `symkey-read.js` MODE=sym | 54,082,799 | 38,139,751 | -34% |
| `symkey-read.js` MODE=str (control) | 48,902,018 | 48,873,976 | 0% |
| `wk.js` | 38,331,566 | 38,025,721 | -1% |
| `proxy-trap-key.js` MODE=sym | 107,559,541 | 84,863,485 | -23% |

MODE=sym now costs less than the string control. `wk.js` only changes slightly because well-known symbols already used
the precomputed table.

JetStream / micro drivers at `--iterations 1` (proxy-* medians of 5; their Ir varies ~+-2% run to run):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| proxy-mobx (median) | 785,622,215 | 782,296,574 | -0.4% |
| proxy-vue (median) | 1,486,774,157 | 1,483,834,933 | -0.2% |
| fib | 7,530,577,573 | 7,554,822,869 | +0.3% |
| richards | 13,858,801,499 | 13,883,556,176 | +0.2% |
| delta-blue | 9,110,187,042 | 9,137,059,621 | +0.3% |

Callgrind of proxy-mobx on main: `JsSymbol::to_property_key` 8.99M inclusive (1.14%) and `Interpreter::to_property_key`
12.26M (1.56%); with the memo they are 0 and 3.43M (0.44%). The 6.04% the issue measured at `2ea6417` no longer exists
on main, so the issue's proxy-mobx >= 5% criterion cannot be met any more; the whole remaining ceiling was ~1.1% and
it is gone, inside the noise of the total.

The controls sit +0.2..0.3% above base after the `vm.rs` change. fib never touches a Symbol, so this is LTO layout
noise, not an executed-path cost.

## Correctness gates

`test262-extra/Proxy-trap-symbol-key-identity-and-description.js` (cites the Proxy `[[Get]]` algorithm) checks identity
and description of a Symbol key through the get/set/has/delete/defineProperty/getOwnPropertyDescriptor/ownKeys traps for
undefined, empty, lone-surrogate and well-known-looking descriptions, and the description of a key whose Symbol was
dropped (after `$262.gc()`). Unit tests cover memo reuse, the round trip, weak (non-retaining) behaviour, id reuse and
the sweep.
