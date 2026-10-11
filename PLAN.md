# Plan: cheap function-object creation — pre-built keys, no spill, then (conditionally) lazily materialised own properties (#901)

## Problem restated

`Interpreter::create_function` (`src/interpreter/mod.rs:1819-1984`) eagerly builds every function object's own
properties: `length`, `name`, (`caller`, `arguments` for sloppy plain functions), `prototype`, plus a second heap
object (the `.prototype` with a `constructor` back-link), a `class_name` `String`, a `JsString` name (3
allocations), and a `function_realm_map` hash insert. Each `insert_property` re-interns the key (SipHash of the bytes
through the thread-local `KEY_CACHE`), pushes `property_order` (Vec grows 0→4→8), and the 5th property of a sloppy
constructable function — i.e. the benchmark's `function (y) { return x + y; }` — spills the `PropertyMap` into a
SipHash `HashMap`. Baseline (`docs/perf/2026-10-10/repros/calls/findings.md` RC3, measured before #900/#954): ~14.3k
instr per closure creation, `bench_calls_closure_create` 5.0x boa; each Promise capability pair costs 2x
`create_function` (~4.1k instr each). Goal: ≤ 5k instr per closure creation and `bench_calls_closure_create`
ratio ≤ 3.0 with **no observable change** to function own-property keys, order, attributes or realm behaviour.

## Spec basis

No JavaScript behaviour changes; this is a representation change that must preserve the observable results of
these clauses. Clause numbers are from a pinned `spec/spec.html` checkout (`spec/` is an uninitialised submodule in
this workspace — run `git submodule update --init --depth 1 spec test262` and re-verify numbers; the `sec-*` ids are
the stable handles):

- 10.1.11.1 OrdinaryOwnPropertyKeys (`sec-ordinaryownpropertykeys`) — integer indices ascending, strings in
  creation order, symbols in creation order; this is what makes materialisation order observable
  (`Reflect.ownKeys`, `Object.getOwnPropertyNames`, for-in).
- 10.2.3 OrdinaryFunctionCreate (`sec-ordinaryfunctioncreate`, ends with SetFunctionLength → `length` first),
  10.2.9 SetFunctionName (`sec-setfunctionname`), 10.2.10 SetFunctionLength (`sec-setfunctionlength`),
  10.2.5 MakeConstructor (`sec-makeconstructor`: `prototype` {w:true,e:false,c:false}, `constructor`
  {w:true,e:false,c:true}), 10.2.4 / 10.2.4.1 AddRestrictedFunctionProperties and %ThrowTypeError%
  (`sec-addrestrictedfunctionproperties`, `sec-%throwtypeerror%`: non-configurable `length`/`name`, non-extensible).
- 10.3.4 CreateBuiltinFunction (`sec-createbuiltinfunction`: `length` then `name`), 10.4.1 bound function exotic
  objects (`sec-bound-function-exotic-objects`, no own `prototype`), 10.5.11 Proxy [[OwnPropertyKeys]]
  (`sec-proxy-object-internal-methods-and-internal-slots-ownpropertykeys`).
- 15.7.14 ClassDefinitionEvaluation (`sec-runtime-semantics-classdefinitionevaluation`: class `prototype` is
  {w:false,e:false,c:false} from the start; static `name`/`length` members overwrite the existing slot in place);
  27.3.4 / 27.4.4 / 27.7.4 GeneratorFunction / AsyncGeneratorFunction / AsyncFunction instances
  (`sec-generatorfunction-instances`, `sec-asyncgeneratorfunction-instances`, `sec-async-function-instances`:
  generators own `prototype` with no `constructor`).
- 17.1 Forbidden extensions (`sec-forbidden-extensions`): strict functions, methods, generators and async functions
  must not own `caller`/`arguments`; sloppy plain functions owning them is implementation-defined — jsse's current
  `length, name, caller, arguments, prototype` order is preserved, not "fixed" toward V8.
- 7.3.24 GetFunctionRealm (`sec-getfunctionrealm`), 10.2.1.1 PrepareForOrdinaryCall (`sec-prepareforordinarycall`),
  20.2.3.2 Function.prototype.bind (`sec-function.prototype.bind`: HasOwnProperty + Get of `length`, Get of `name`),
  20.2.1.1.1 CreateDynamicFunction (`sec-createdynamicfunction`).

## Assumptions

- **Phase 2 (lazy materialisation) is conditional.** The run contract asks for the minimal slice that closes the
  issue. Phase 1 is planned unconditionally; Phase 2 proceeds only if the go/no-go rule after slice 5 says so.
  Phase 2 as designed still allocates the second `JsObjectData`-sized prototype object eagerly (allocating it needs
  `&mut Interpreter`, which a borrow-time hook lacks); its gain is the own-property inserts only, weighed against one
  predictable `Cell<bool>` branch added to every `ObjectHandle::borrow`/`borrow_mut`.
- **Measurement.** This host has neither `valgrind` nor `perf` (`perf_event_paranoid=2`, no binary), so the ≤5k
  Ir criterion cannot be verified here. The implementer first runs
  `cat /proc/sys/kernel/perf_event_paranoid; command -v perf valgrind`. If `valgrind` exists: callgrind recipe
  `findings.md:182-200` (scaled to 10,000 iterations, startup subtracted). Else if `perf` works:
  `perf stat -e instructions:u` at N and 2N iterations, `(Ir(2N) − Ir(N)) / N`. Else: the gates are the
  deterministic counters from slice 2 plus 5 interleaved wall-clock medians against a baseline binary snapshotted to
  `$TMPDIR` before the first change, and the PR states plainly that Ir was not measured.
- Baseline must be re-measured at HEAD `9703e359`: #900 (shared bytecode verdict) and #954 (atom environments)
  landed after the 14.3k figure.
- `INLINE_CAP` stays 4 and `size_of::<JsObjectData>()` must stay ≤ 3072 (tripwire
  `js_object_data_size_under_3kb`); the spill problem is solved by decoupling the HashMap threshold (slice 3).
- `class_name: String` → `Cow<'static, str>` (153 assignment sites) is a contingency only: nothing compares function
  `class_name`s, so the saving is one small malloc per object.
- Do **not** "skip the `function_realm_map` insert for the default realm" as the issue suggests: on a map miss the
  call path leaves `current_realm_id` (the caller's realm) unchanged, so a realm-0 function called from another realm
  would run in the wrong realm. Replaced by an object field with the same `None` fallback (slice 5).

## Files to touch

| File | Role | Lines of Interest |
|------|------|-------------------|
| `src/interpreter/property_map.rs` | decouple HashMap-spill threshold from `INLINE_CAP`; update spill tests | 8-13, 32, 75-104, 191-346 |
| `src/interpreter/mod.rs` | `create_function(_with)`, `create_thrower_function`, `set_function_name`, `function_realm_map` field/init, `get_function_realm` | 306, 742, 1580-1610, 1802-1984, 2187-2215 |
| `src/interpreter/types.rs` | `JsObjectData` (insert wrappers, new fields), `Realm.sloppy_*_getter` | 598-599, 827-834, 2042-2100, 2409-2440, 2524-2540, 3480-3530 |
| `src/interpreter/key_intern.rs` | seed `caller`/`arguments`; `key_intern_calls` counter | 49-75, 127-140 |
| `src/interpreter/perf_counters.rs` | new counters + report lines | 1-240 |
| `src/interpreter/body_memo.rs`, `src/interpreter/eval.rs`, `exec.rs`, `eval/literals.rs`, `mod.rs` | memoise per-Body `(Rc<params>, uses_arguments, has_simple_params)`; user-closure creation sites | eval.rs:594-632, 6311-6326; exec.rs:591-606, 2707-2722; mod.rs:5168-5232, 5398-5413; literals.rs:381-409, 618-633, 797-815 |
| `src/interpreter/builtins/function.rs`, `builtins/proxy.rs`, `eval/literals.rs` | stop create-then-strip (bound, Proxy ctor, methods, class ctor `prototype`) | function.rs:187-260; proxy.rs:40-110; literals.rs:414-431, 1488 |
| `src/interpreter/eval.rs`, `eval/modules.rs`, `builtins/function_constructors.rs`, `gc.rs` | `function_realm` field replaces `function_realm_map` | eval.rs:5287, 5328, 5369; modules.rs:651; function_constructors.rs:350, 379; gc.rs:800 |
| Phase 2 only: `src/interpreter/object_arena.rs`, `types.rs`, `mod.rs`, `eval.rs` (call dispatch/call IC), `bytecode/vm.rs:735-760`, `helpers.rs:397`, `builtins/promise.rs:2234`, `gc.rs:583-590, 937-950` | materialise-on-borrow hook, non-materialising borrow, GC edges | object_arena.rs:61-110, 215-224 |
| `tests/perf_counters_function_creation.rs` [new] | counter gates (child-process pattern of `tests/perf_counters_env_hashing.rs`) | — |
| `test262-extra/function-own-keys-order-and-attributes.js` [new], `function-own-props-define-delete-freeze.js` [new], `function-lazy-props-gc-rooting.js` [new, Phase 2] | spec-correct behaviour not in test262 | — |
| `src/interpreter/tests.rs` | exact jsse order pin `length, name, caller, arguments, prototype` | near :3326 |
| `docs/perf/<date>/function-creation.md` [new], `benchmarks/scripts/bench_closure_create.js` [new], `bench_promise_new.js` [new] | measurement record, scaled repro copies | — |
| Phase 2 only: `docs/adr/2026-10-12-1200-lazy-function-own-properties.md` [new], `CONTEXT.md`, `AGENTS.md` | ADR, vocabulary (**Lazy Function Properties**, **Materialisation**, **Non-materialising Borrow**), Architecture Notes bullet | — |

## TDD slices (one commit each, in order)

### Phase 1 — cheap creation, no new lazy state

#### 1. Characterise current function own-property behaviour (green on HEAD)
- **Tests**: `test262-extra/function-own-keys-order-and-attributes.js`,
  `test262-extra/function-own-props-define-delete-freeze.js`, Rust pin in `src/interpreter/tests.rs`.
- **Change**: tests only. `…-order-and-attributes.js`: for plain / strict / arrow / method / getter / setter /
  generator / async / async-generator functions, class (with/without `extends`, with `static name(){}`,
  `static length = 1`, `static m(){}`), bound function, native builtin, Promise resolve function,
  `Function(...)`-created function and a `$262.createRealm()` function, assert spec-mandated relative order
  (`length` < `name` < `prototype`, sec-ordinaryfunctioncreate/sec-setfunctionname/sec-makeconstructor), absence of
  `prototype` on arrows/methods/async/bound, each descriptor's w/e/c, `prototype.constructor === f` (not on
  generators), generator `prototype` [[Prototype]] and absence of `constructor`, class `prototype` non-writable.
  `caller`/`arguments` asserted only as "if own, non-enumerable and configurable" (implementation-defined).
  `…-define-delete-freeze.js`: as the *first* operation on a fresh function perform `delete f.name`,
  `Object.defineProperty(f,'length',…)`, `Object.freeze(f)`, `Object.preventExtensions(f)` then `Reflect.ownKeys`;
  also `getOwnPropertyDescriptors`, `hasOwnProperty`/`in`, for-in, `new Proxy(f,{})` ownKeys, `bind` reading
  `length`/`name`. The Rust test pins the exact engine order for a sloppy function (engine-specific).
- **Reuses**: header + `Test262Error` pattern of `test262-extra/property-removal-clears-ownkeys-order.js`.

#### 2. Deterministic creation counters (red gate for slice 3)
- **Files**: `src/interpreter/perf_counters.rs`, `src/interpreter/key_intern.rs`, `src/interpreter/mod.rs`,
  `tests/perf_counters_function_creation.rs` [new].
- **Change**: under `perf-counters` add `function_objects_created`, `function_prototype_objects_created`,
  `function_props_materialised` (0 until Phase 2), `key_intern_calls` (thread-local `Cell<u64>` bumped in
  `intern_key`/`intern_js_key`). Report as `PERF\t<name>\t<n>`.
- **Test (red)**: create N then 2N closures `function(y){return x+y}` in a loop and call each once → assert
  `key_intern_calls` is equal for both sizes while `function_objects_created` grows. Fails on HEAD (≥6 interns per
  closure).
- **Reuses**: `counters()` helper of `tests/perf_counters_env_hashing.rs:15-47`.

#### 3. Fresh-property push with pre-built keys; no spill below 9 entries
- **Files**: `src/interpreter/property_map.rs`, `src/interpreter/types.rs`, `src/interpreter/mod.rs`,
  `src/interpreter/key_intern.rs`.
- **Change**:
  - `property_map.rs`: add `LINEAR_MAX = 8`; keep `INLINE_CAP = 4` for the SmallVec's inline storage but convert to
    `HashMap` only when `len` would exceed `LINEAR_MAX` (SmallVec heap-spills for 5–8 entries: one malloc, linear
    scan, no SipHash). `size_of` unchanged. Update the spill tests (`one_past_capacity_triggers_spill`,
    `remove_after_spill_keeps_spilled`, iteration/Symbol-key tests) to use `LINEAR_MAX`.
  - `types.rs`: `JsObjectData::push_fresh_property(key: JsPropertyKey, desc)` — appends to `properties` and
    `property_order` with no `intern_js_key` and no `contains_key` (debug-assert absent);
    `JsObjectData::with_class_name(&str)` so the function path allocates the class name once.
  - `mod.rs`: `FunctionKeys { length, name, caller, arguments, prototype, constructor }` built once in
    `Interpreter::new` from `intern_key`; `create_function` uses it (also for the prototype object's `constructor`),
    reserves `property_order` capacity 5, and shares one `Interpreter.empty_js_string` (Arc clone) for anonymous
    names. Order and descriptors unchanged.
- **Passes**: slice 2 test, slice 1 tests unchanged; add `bench_object.js` to the no-regression runs.

#### 4. Stop creating-then-stripping
- **Files**: `src/interpreter/mod.rs` (`create_function_with(func, FunctionInit { with_prototype,
  class_prototype_writable })`; `create_function` is the default wrapper; add `!is_method` to the
  caller/arguments condition), `src/interpreter/builtins/function.rs:187-260` (bound),
  `src/interpreter/builtins/proxy.rs:59`, `src/interpreter/eval/literals.rs:414-431, 1488` (class ctor creates
  `prototype` non-writable directly; the method strip goes away).
- **Passes**: `test262-extra/no-own-prototype-caller-arguments-leak.js`, slice 1 tests,
  `test262/test/built-ins/Function/prototype/bind`. Bound functions stop allocating a prototype object.

#### 5. Per-Body derived data and `function_realm` as an object field; **go/no-go**
- **Files**: `src/interpreter/body_memo.rs` + the 11 creation sites (`exec.rs:603/2719`, `eval.rs:606/6323`,
  `mod.rs:5180/5229/5410`, `literals.rs:381/630/811`); `JsObjectData.function_realm: Option<u32>`.
- **Change**: memoise `(Rc<Vec<Pattern>>, uses_arguments, has_simple_params)` per Body in `BodyMemo` (no AST/parser
  change), replacing per-closure `Rc::new(params.clone())` + `func_uses_arguments` + `params_are_simple`. First
  verify no two function literals with different params share a `Body::key()` (arrows build `ArrowBody::Expression`
  bodies — confirm their `Rc` is stable per literal; the #947 synthesized default constructors share Body and params,
  which is fine). Fall back to keeping per-closure computation for any literal kind that cannot be keyed.
  Replace `function_realm_map` reads/writes/removal with the field; `None` falls back to `current_realm_id` exactly
  as the map miss does today.
- **Gate**: `cargo test` (incl. `js_object_data_size_under_3kb`); `built-ins/Function`, `built-ins/ShadowRealm`,
  `language/expressions/{function,class}`, `$262.createRealm`-based tests.
- **Go/no-go for Phase 2**: measure per the Assumptions. Proceed to Phase 2 **only if** instr per closure creation
  is still > 5k or `bench_calls_closure_create` is still > 3.0x boa (when Ir/boa unavailable: wall-clock reduction vs
  the HEAD baseline binary is < 40%, which is the 5.0x→3.0x equivalent). If met, stop here, record numbers in
  `docs/perf/<date>/function-creation.md`, list Phase 2 as a follow-up issue.
- **Contingencies** (only if still over budget and Phase 2 is declined): `JsFunction::User.name` →
  `Option<Rc<str>>`; `class_name` → `Cow<'static, str>`.

### Phase 2 — lazily materialised function own properties (materialise-on-borrow; conditional)

#### 6. ADR + vocabulary
- **Files**: `docs/adr/2026-10-12-1200-lazy-function-own-properties.md` [new], `CONTEXT.md`.
- **Change**: record the design below and the Non-materialising Borrow contract.

#### 7. Red tests for laziness
- **Files**: `tests/perf_counters_function_creation.rs`, `test262-extra/function-lazy-props-gc-rooting.js` [new].
- **Change**: creating and calling 1,000 closures → `function_props_materialised == 0`; closures passed as
  callbacks to `Array.prototype.map`, `Promise.prototype.then`, `setTimeout`, plus `typeof f`, `f === g`, and Promise
  capability pairs → still 0; `f.name` / `Object.keys(f)` / `f.prototype` / `new f()` → exactly 1 per distinct
  function. **Not** in the zero list: `f.call/apply/bind`, `instanceof` (these do a property get on `f`, which goes
  through the hooked borrow). GC test (`*-gc-rooting.js` so CI's `JSSE_GC_STRESS=7` pass runs it): create closures,
  keep only the closure, `$262.gc()` repeatedly, then check `f.prototype.constructor === f` and `Reflect.ownKeys(f)`.

#### 8. The hook
- **Files**: `src/interpreter/object_arena.rs`, `src/interpreter/types.rs`.
- **Change**: `ObjectAllocation.lazy_fn: Cell<bool>`; `JsObjectData.lazy_function_props:
  Option<LazyFunctionProps { proto_id: Option<u64>, template: Rc<FunctionPropTemplate> }>`. **Hook only
  `ObjectHandle::borrow` and `borrow_mut`** (`#[cold] #[inline(never)] materialize_function_props`);
  `borrow_untracked`/`borrow_mut_untracked` are the GC's and collections' own path (8 sites in `gc.rs`, 36 total) and
  must not materialise — audit all 36 (none may read function own properties; `property.rs:1075,1232` and
  `env_helpers.rs:109,137` operate on ordinary/global objects). New `ObjectHandle::borrow_raw()` (shared, never
  materialises) for non-property reads. Materialisation: takes the lazy state, then via a tracked `borrow_mut`
  (write barrier) pushes in order `length`, `name`, [`caller`, `arguments`], [`prototype`] using the template's
  pre-built keys, derived from `callable` (`length` = params before first `Assign`/`Rest` or native arity; `name`
  from `JsFunction`; caller/arguments accessors or `null` data exactly as today, captured in the template at
  creation), clears the flag, bypasses the extensibility check, and **bumps `shape_id`** (once per function; a
  call-IC entry misses once and re-records, and no stale ProtoData/Missing entry for `Function.prototype`'s `name`/
  `length` can survive materialisation).
- **Passes**: compile-green, all existing tests green (nothing sets the flag yet).

#### 9. Create functions lazy; convert hot internal paths to `borrow_raw`
- **Files**: `src/interpreter/mod.rs` (`create_function_with` sets the lazy state, allocates the prototype object and
  its `constructor` back-link eagerly; `Realm.function_prop_template: Option<Rc<…>>` invalidated when
  `function_constructors.rs:400/423` assign the sloppy getters), `src/interpreter/eval.rs` (call dispatch ~:5183,
  call-IC `classify_for_call_ic`/probe ~:4905-4997), `bytecode/vm.rs:735-760`, `helpers.rs:397` (`typeof_val`),
  `builtins/promise.rs:2234` (`is_callable`), `get_function_realm`, `gc.rs` (`trace_object_fields` traces
  `lazy_function_props.proto_id` and the template getters; `object_requires_persistent_minor_scan` also true when
  the flag is set; `gc.rs` must use only non-hooked accessors — audit its 19 `borrow()/borrow_mut()` sites).
- **Audit rule**: every `.callable` read (69 outside tests, `grep -rn "\.callable" src`) stays on the hooked borrow
  (correct, materialises; acceptable off the hot path) or moves to `borrow_raw` when it only reads
  `callable`/`kind`/`prototype_id`. `classify_for_prop_ic` and `ic_probe_prop_kind` (`eval/access.rs`) must **never**
  use `borrow_raw`. A `borrow_raw` `Ref` must not be alive across a hooked borrow of the same object (the hook's
  `try_borrow_mut` fails → panic with a clear message; caught by `cargo test` and the release-checked run).
- **Passes**: slice 7 green; `function_props_materialised == 0` on the scaled `bench_closure_create.js` and
  `bench_promise_new.js`.

#### 10. Measure and document
- **Files**: `docs/perf/<date>/function-creation.md`, `AGENTS.md`.
- **Change**: before/after per closure and ratios; no-regression (±1%) on `benchmarks/scripts/bench_opmix.js`,
  `bench_object.js`, `bench_closures.js` for the extra branch in `borrow`.

## Test surface

- Per slice: `cargo test` (debug asserts incl. size tripwire), `./scripts/lint.sh`.
- test262 targeted (after slices 3, 5, 9): `built-ins/{Function,GeneratorFunction,AsyncFunction,
  AsyncGeneratorFunction,ThrowTypeError,Reflect,Proxy,ShadowRealm,Promise}`, `built-ins/Object/{getOwnPropertyNames,
  keys,entries,values,getOwnPropertyDescriptor,getOwnPropertyDescriptors,defineProperty,defineProperties,freeze,
  isFrozen,seal,isSealed,preventExtensions,prototype/hasOwnProperty}`, `language/expressions/{function,arrow-function,
  class,generators,async-function,async-generator,object/method-definition,new,instanceof,delete}`,
  `language/statements/{function,class,generators,async-function,async-generator,for-in}`,
  `language/{function-code,arguments-object,eval-code}`, `annexB/built-ins/Function`,
  `annexB/language/{function-code,eval-code,global-code}`.
- Full before PR: `uv run python scripts/run-test262.py` (never `--update-baseline`), then
  `uv run python scripts/run-custom-tests.py`.
- Not covered by test262 → `test262-extra/` (slices 1, 7, naming the spec clauses in the header) and
  `tests/perf_counters_function_creation.rs` (counters; run with `--features perf-counters`).
- GC / rooting: `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --timeout 300` normal and
  `--no-bytecode`; `JSSE_GC_STRESS=1` on the new files; `cargo build --profile release-checked` +
  `--binary target/release-checked/jsse` over `test262-extra/` and a 10% `--sample`.
- Library harnesses (closure-heavy): `./scripts/run-library-tests.sh acorn`, `decimal.js`.
- Prereq: `git submodule update --init --depth 1 test262 spec`.

## Regression risk

- **`test262-pass.txt` movers**: any change in own-key order/presence of function objects
  (`built-ins/Function/**/property-order.js`, `built-ins/ThrowTypeError/property-order.js`, class
  `fn-name-static-precedence-order.js` / `fn-length-static-precedence-order.js`, `prototype-own-properties.js`),
  realm-switch behaviour (`proto-from-ctor-realm*.js`, `built-ins/ShadowRealm`), Proxy ownKeys invariants.
- **Shared machinery leaned on**: the property MOP in `property.rs` (`proxy_own_keys` reads `property_order`;
  `get_property_on_id`, `define_object_property`, `delete_object_property`); direct `.properties` (~125) and
  `.property_order` (~17) readers that bypass any `JsObjectData`-level hook; `set_function_name` (`mod.rs:2187`,
  reads `properties.get("name")` then inserts — correct in Phase 2 only because the hooked borrow materialises
  first); `%ThrowTypeError%` (`builtins/mod.rs:99-118`: `extensible = false` then re-insert); `eval_expr`/
  `exec_statement` hot paths (closure creation sites; call dispatch at `eval.rs:5183/5287-5369`); bytecode
  call IC (`vm.rs:735-760`, caches `callee.shape_id`); GC rooting and `gc_safepoint()` (`trace_object_fields`,
  persistent minor scan, LIFO `gc_temp_roots` — creation allocates no objects after the function object, no new
  temp roots); exhaustive `ObjectKind` matches (**no new `ObjectKind` variant is added**, so none change);
  Node-compat library harnesses.
- **Realm**: template/getters captured from the *current* realm at creation (as today); `function_realm` fallback
  stays `current_realm_id`.
- **Size tripwire**: `lazy_function_props` (~24 B) + `function_realm` (8 B) may exceed 3072 B — trim a field (e.g.
  box `private_fields`) rather than raising the cap.
- **Non-function callables** (`Function.prototype`, `$262.IsHTMLDDA`, wrapped functions) are not created by
  `create_function`; never lazy — do not infer laziness from `callable.is_some()`.
- **Performance regression risk (Phase 2)**: extra branch on every `borrow`; confirm ±1% on object-heavy benches.
- Pre-existing quirk, not to be changed: `var C = class { static name = "" }` is renamed by the empty-string
  heuristic in `set_function_name`.

## Out of scope

- Lazy *prototype object* allocation (needs an interpreter-level hook) — follow-up issue.
- Arguments-object cost (`create_arguments_object`), remaining per-call environment work (mostly #954), promise
  machinery (`then()` without resolving functions, async state-machine caching) — other #897 items.
- Changing `caller`/`arguments` order or presence on sloppy functions; fixing the `static name = ""` heuristic.
- Raising `INLINE_CAP`; swapping the spill hasher (PR #393 decision); AST/parser node changes.
- `test262-pass.txt` baseline updates (main-only operation).
