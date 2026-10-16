# #1062: keep parameter atoms and the IC store handle per literal, not per call

A compiled call re-derived two facts that are pure functions of the function literal and threw them away.

1. `bind_simple_parameters` looked the Body up in `frame_layout_memo` (clock tick, FxHash of the Body pointer, probe, `Rc`
   clone), re-proved the memo by comparing every parameter `String` in the AST with its atom, then bound each parameter
   with `BindingMap::insert_atom` (a `find_in` scan of the bindings so far).
2. `dispatch_body` -> `with_ic_body` -> `IcStore::enter_body` hashed the same Body pointer again.
3. `dispatch_body` fetched the compile verdict with `get_object(id).borrow().bytecode_cache()` although the call path
   already held that function object's borrow.

## Change

`LiteralFrame` (`interpreter/literal_frame.rs`) is created with a literal's `FunctionLiteralInfo`, shared by `Rc` with
every closure (`JsFunction::User.frame`, copied into `CallTarget::User`), and holds

- the interned parameter atoms (filled on the first call) and whether they are all distinct and none is `arguments`;
- a `Cell<Option<BodyStoreHandle>>` for the literal's IC store.

`bind_simple_parameters` reads the atoms off the frame (`debug_assert!` keeps the old string check in debug builds) and,
when the names are fresh, appends with `push_unique`; duplicate names and a formal named `arguments` keep `insert_atom`.
`IcStore::enter_body_cached` takes the remembered handle when its slot generation matches and the slot's store still
belongs to this Body (`body_key()`), still ticks the recency clock and counts the entry in flight, and otherwise falls
back to `enter_body` and refreshes the cell. `dispatch_body` receives the compile verdict that `CallTarget::User`
already cloned under the call path's borrow. `frame_layout_memo` and `JsObjectData::bytecode_cache()` are gone.
Functions built without a literal (`Function` constructor and its siblings) carry their own frame; the synthesized
default class constructors share one thread-local frame like they share one Body.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `ede164c0` (binary sha256 `cc2bac2606e2...`),
new = base plus this change (`c66578f24850...`; the committed tree differs only by `cargo fmt`). JetStream drivers at
`--iterations 1`.

| benchmark | base Ir | new Ir | delta | criterion |
|---|---:|---:|---:|---|
| fib (micro) | 8,394,884,195 | 7,985,615,821 | -4.88% | >= 2.0% |
| richards | 14,391,090,388 | 14,129,967,615 | -1.81% | >= 1.0% |
| delta-blue | 9,714,688,307 | 9,413,675,207 | -3.10% | >= 1.5% |
| hash-map | 23,748,902,144 | 23,161,403,569 | -2.47% | |
| bigint-noble-ed25519 | 8,144,108,386 | 8,007,838,950 | -1.67% | |
| Babylon | 3,583,057,271 | 3,534,930,844 | -1.34% | |
| proxy-mobx (median of 5) | 823,851,904 | 807,393,669 | -2.00% | noise ~4% |

proxy-mobx's total Ir is not run-to-run stable (randomised interner), so only the median means anything.

Reproducer (the issue's leaf-call script, Ir per iteration = (N=250,000 minus N=50,000) / 200,000):

| | base | atoms + IC handle | + verdict pass-through (shipped) | criterion |
|---|---:|---:|---:|---|
| 1 parameter `inc(s)` | 3,479 | 3,359 (-3.4%) | 3,328 (-4.3%) | <= 3,440 (-2.3% of the issue's 3,522) |
| 4 parameters `inc(s,1,2,3)` | 4,263 | 4,042 (-5.2%) | 4,011 (-5.9%) | <= 4,170 (-3.2% of 4,308) |
| call-free control | 854 | 853 | 853 | within 0.5% |

Megamorphic IC-store eviction path (9,000 `new Function` bodies, then 20,000 calls of one hot function, 20 rounds):
16,161,208,248 -> 16,048,451,864 Ir (-0.7%), wall 4.75 s -> 4.78 s, same output.

By construction the call path no longer touches `BodyMemo<Rc<Vec<Atom>>>::get` or the AST parameter strings, and a
handle hit performs no `index` probe.

## Correctness gates

`test262-extra/call-frame-literal-parameter-binding.js` (closures sharing a literal, duplicate formals, a formal named
`arguments`, eval redeclaring a formal, the same literal evaluated in two realms, `Function`-constructor functions,
default class constructors) and `call-frame-literal-ic-handle-after-eviction.js` (a hot function called again after
9,000 distinct bodies evicted the IC store) pass in both modes and under `JSSE_GC_STRESS=7`. Unit tests in `ic_store.rs`
cover a cached hit (still counted in flight), a handle that went stale through eviction, and a cell that was filled for
another Body; `literal_frame.rs` tests the fresh-name rule. Full test262 default and `--no-bytecode`: 99,911/99,911
each; `test262-extra/` 1,420/1,420 normal, `--no-bytecode`, and both under `JSSE_GC_STRESS=7`; `cargo test --lib` 1,112
passed; `tests/perf_counters_env_hashing.rs` passes; `scripts/run-custom-tests.py` 19/19 (the two huge-string tests need
the longer timeout on a loaded `release-nolto` build); `./scripts/lint.sh` clean.
