# Plan: #905 perf(interpreter): fast path for global and free-variable reads/writes

## Problem restated

A free name that resolves to a *global-object own data property* (every script-level
`var`, every builtin such as `Math`/`Int32Array`, every implicit global) pays the
full generic resolution on every access, in both the bytecode VM and the
tree-walker:

- **Read** (`resolve_identifier`, `src/interpreter/eval.rs:7761`; callee reads via
  `Op::LoadCalleeName`; `typeof` via `resolve_global_getter`, `eval.rs:7863`): a scope
  walk, then `get_own_property` on the global object — one randomised-SipHash probe of
  the (spilled) `PropertyMap` **plus a cloned `PropertyDescriptor`**.
- **Write** (`Op::ResolveName` + `Op::StoreResolvedName`, `vm.rs:499/530`, and every
  tree-walker assignment through `resolve_identifier_ref`/`put_value_by_ref`,
  `eval.rs:7564/7644`): `find_binding_env` walk, `env_check_set_binding` walk +
  `own_has_property`, `proxy_has_property`, a full `proxy_set` [[Set]] MOP, then
  `sync_global_object_binding` (a realm scan). Roughly five or six property-map hashes
  per store; `x++` is worse (`eval_identifier_update`, `eval.rs:2421`, ~8).

The issue proposes a `(shape, slot)` cache in `LoadName`/`ResolveName`. Reading the
code shows that literal design is not available (see Assumptions): `PropertyMap`
(`src/interpreter/property_map.rs`) has no stable slots and `shape_id` is bumped by
only *some* structural mutators. The plan therefore achieves the goal by **removing
redundant work behind a single re-validated own-property probe** (sound by construction,
no invalidation protocol), measures, and only if the acceptance gates are still missed
adds *verified slot hints* (same technique as the `LoadLocal` slot hint, ADR
2026-10-12-0900) to remove the last hash — expected to be needed for the read-only
`typedarray_global` gate, so slice 2 ends with a mandatory checkpoint that decides it early.

Acceptance (issue): `bench_numeric_globalvars` ≤ 2.3 s (3.18 s base on buildbox3, i.e.
≤ 0.72× base); `bench_numeric_typedarray_global` within 1.15× of
`bench_numeric_typedarray_local`; test262 unchanged.

## Spec basis

Pure performance change: every clause below must keep *exactly* the observable behaviour
it has today; they are the clauses the new tests pin. Cited by `sec-` anchor and name
(section numbers shift between editions; anchors verified present in `spec/spec.html`
at the pinned submodule commit):

- Global Environment Records (`sec-global-environment-records`): `HasBinding`
  (`sec-global-environment-records-hasbinding-n`), `SetMutableBinding`
  (`...-setmutablebinding-n-v-s`), `GetBindingValue` (`...-getbindingvalue-n-s`),
  `DeleteBinding` (`...-deletebinding-n`), `CreateGlobalVarBinding`
  (`sec-createglobalvarbinding`): declarative record is consulted before the object record.
- Object Environment Records (`sec-object-environment-records`): `HasBinding`,
  `SetMutableBinding` (re-checks `HasProperty`, then `Set`, strict-mode `ReferenceError`
  when missing), `GetBindingValue`; the global object is the binding object.
- `GetIdentifierReference` (`sec-getidentifierreference`), `ResolveBinding`
  (`sec-resolvebinding`), `GetValue` (`sec-getvalue`), `PutValue` (`sec-putvalue`):
  reference resolved before the RHS runs, re-validated at PutValue.
- `OrdinaryGet` (`sec-ordinaryget`), `OrdinarySet` (`sec-ordinaryset`),
  `OrdinarySetWithOwnDescriptor` (`sec-ordinarysetwithowndescriptor`): data / accessor /
  non-writable own property; a Proxy is the only observable difference.
- Identifier evaluation (`sec-identifiers-runtime-semantics-evaluation`), assignment
  (`sec-assignment-operators-runtime-semantics-evaluation`), postfix/prefix update
  (`sec-postfix-increment-operator`), `typeof` (`sec-typeof-operator`), `delete`
  (`sec-delete-operator`), `with` (`sec-with-statement`).
- `GlobalDeclarationInstantiation` (`sec-globaldeclarationinstantiation`) and
  `EvalDeclarationInstantiation` (`sec-evaldeclarationinstantiation`): var/function/lexical
  declarations against existing global properties; later `let` shadowing a configurable
  global property.
- Multiple realms: ecma262 realm records; `ShadowRealm` is a separate TC39 proposal
  (not in ecma262) — each realm has its own global object/environment; the fast path takes
  the global object id from the *resolved* environment, never from `current_realm`.

## Assumptions

- **No literal `(shape, slot)` IC first** (best guess, from code): `PropertyMap`
  spills to a `HashMap` with no slot index, and `JsObjectData::remove_property`
  (`types.rs:3526`), `insert_property` (`types.rs:3508`) and the direct
  `properties.insert` calls in `env_helpers.rs:225/253/337` do **not** bump `shape_id`
  (only `mutate_object_shape`, `set_property_value`'s add branch and a few array paths
  do). A shape-keyed cache would be unsound without auditing ~30 mutation sites.
  Re-validating one own-property probe per access has no invalidation surface.
- Script-level `var` is declared **only on the global object** (`env_declare_global_var`,
  `env_helpers.rs:217`; `exec.rs:246`), not in `global_env.bindings`. Function
  declarations, `let`/`const`/`class`, implicit globals and eval-declared vars live in
  `global_env.bindings` (the latter two also mirrored on the object). The fast path must
  therefore apply when `global_env.bindings` *lacks* the name and otherwise leave the
  existing path untouched.
- Confirmed in code: compiled **Script** chunks run through `run_script_chunk` with
  `declare_chunk_vars = false` (`vm.rs:219-225`, caller `exec.rs:119`) after
  `instantiate_body_declarations`, so script `var`s are never `declare_atom`'d into
  `global_env.bindings`; only function-body chunks (`run_chunk`, `vm.rs:206`) declare into
  their own function env. The guards "`bindings` lacks the name" therefore target the
  right case.
- The issue's measured numbers predate #898/#953/#954/#956; the plan re-baselines
  (slice 0) and evaluates acceptance as **same-host, interleaved, median-of-5 ratios**
  (precedent: `docs/perf/2026-10-12/898-atom-environments.md`), not absolute seconds.
- Skipping the intermediate-scope walk (hop count + epoch/poison flag) is deliberately
  **not** planned: closures of a shared Body (`273301c8` synthesized default ctors),
  sloppy direct-eval `var` injection and `with` make a static-hop cache need a
  permanent "dynamic scope seen" poison flag plus a static-structure argument. Follow-up
  only.
- `typeof ident`, `delete ident`, `with` and module-import paths keep their current
  structure; they only inherit the cheaper `resolve_global_getter` read.
- The issue's `eval.rs:7696` reference is stale (now `eval.rs:7761`).

## Files to touch

| File | Role | Lines of interest |
|------|------|-------------------|
| `src/interpreter/global_access.rs` [new] | Deep module: `global_object_own_data` (read) and `global_object_store_own_data` (write) — the only place that decides "ordinary, non-proxy, own, data, writable" | new |
| `src/interpreter/mod.rs` | `mod global_access;` | module list |
| `src/interpreter/eval.rs` | `resolve_identifier` step 4 (7822-7858), `resolve_global_getter` (7863-7905), `put_value_by_ref` `SpecificEnv` arm (7658-7740), `sync_global_object_binding` (7632), `eval_identifier_update` (2421) | see left |
| `src/interpreter/bytecode/vm.rs` | `load_name_value` (27-47), `Op::LoadName` 333, `Op::LoadCalleeName` 481, `Op::ResolveName` 499, `Op::StoreResolvedName` 530, `Op::UpdateName` 561 | callers only; phase-2 hint plumbing |
| `src/interpreter/env_helpers.rs` | `env_set` (42), `env_check_set_binding` (267): semantic reference for the write fast path (barrier: `gc_write_barrier_value` + `borrow_mut_untracked().set_property_value`) | 42-165 |
| `src/interpreter/property.rs` | `proxy_set_with_outcome_and_proxy_depth` own-data branch (~1060-1075): the exact write the fast path must equal | 897-1075 |
| `src/interpreter/types.rs` | `get_own_property` (2771) clones; `own_property_lookup` (3554); `set_property_value` (3283); `JsObjectData` size tripwire (`property_map.rs` test) | see left |
| `src/interpreter/gc.rs` | `gc_write_barrier_value` (355) | 355-368 |
| `src/interpreter/perf_counters.rs` | add `global_prop_fast_hits/_misses` next to `local_slot_hits` (75) and the report line (229) | 75, 115, 229 |
| `src/interpreter/property_map.rs` | phase-2 only: slot-addressable `Spilled` storage | 25-132 |
| `src/interpreter/bytecode/chunk.rs`, `compiler.rs` | phase-2 only: `Chunk.global_hints: Vec<Cell<u32>>` parallel to `names` | chunk.rs 17-27 |
| `src/interpreter/tests.rs` | whitebox tests for the helpers | near 2578 (shape-id tests) |
| `test262-extra/*.js` [new] | semantic regressions (below) | — |
| `tests/perf_counters_global_access.rs` [new] | deterministic proof the fast path is taken | model: `tests/perf_counters_env_hashing.rs` |
| `docs/perf/2026-10-12/905-global-reads.md` [new] | before/after medians, callgrind counts | model: `898-atom-environments.md` |
| `docs/adr/2026-10-12-1500-global-object-data-fast-path.md` [new] | decision record (no shape IC; re-validation; why no hop-skip) | model: ADR 2026-10-12-0900 |
| `CONTEXT.md`, `AGENTS.md` | term **Global Data Fast Path**; source-layout line for `global_access.rs`; perf-counter names | Language / Source Layout |

## TDD slices

### 0. Baseline and counters scaffolding (no behaviour change)
- **Do**: build `cargo build --release -j4` and `CARGO_TARGET_DIR=$TMPDIR/target-pc cargo build --release --features perf-counters -j4`. Run the four repros from
  `docs/perf/2026-10-10/repros/numeric/` (+ `bench_calls_global_loop`, `benchmarks/scripts/bench_loop.js` as a
  no-global control) interleaved, median of 5, default and `--no-bytecode`. Run a scaled-down copy of
  `bench_numeric_globalvars.js` under `valgrind --tool=callgrind` and record the top-10 inclusive costs.
- **Red test**: `tests/perf_counters_global_access.rs` [new] (`#![cfg(feature = "perf-counters")]`,
  child-process like `perf_counters_env_hashing.rs`) asserts the stderr report contains
  `PERF\tglobal_prop_fast_hits` and `PERF\tglobal_prop_fast_misses` (fails: not reported).
- **Green**: add the two counters to `perf_counters.rs` (struct 75, init 115, report 229). Nothing
  increments them yet; the "grows with iterations" assertions arrive with the slice that makes them
  true (reads in slice 2, stores in slice 3).
- Result is recorded in `docs/perf/2026-10-12/905-global-reads.md`.

### 1. Characterisation tests that must be green on HEAD *before* any engine change
Commit these first (they prove behaviour preservation). Each is a `test262-extra/<name>.js`
with `/*--- description / info / esid / flags ---*/`, warms the site in a loop (≥ 3 iterations so any
cache would be populated), then mutates the global and re-reads/re-writes; `flags: [noStrict]`
or `[onlyStrict]`/`[raw]` as needed; use `$262.evalScript`/`$262.createRealm()` where a second script/realm
is needed. Existing neighbours to extend rather than duplicate:
`ordinary-set-global-binding-sync.js`, `identifier-resolution-global-function-callee-in-large-global-env.js`,
`direct-eval-var-in-function-with-many-bindings.js`, `with-shadows-local-in-uncompiled-function.js`.
1. `global-var-read-write-through-global-object-alias.js` — §9.1.1.2.5/§9.1.1.2.6: bare write ↔ `globalThis.x` read and the reverse, inside functions and at top level, `x++`, `x += 1`, `typeof x`.
2. `global-var-nonwritable-after-warm.js` — §10.1.9.2: `Object.defineProperty(globalThis,"x",{writable:false})` after warm → sloppy silent, strict `TypeError`; `Object.freeze(globalThis)`.
3. `global-property-delete-after-warm.js` — §9.1.1.4.7/§9.1.1.2.5: `delete globalThis.y` (configurable) → read `ReferenceError`; strict assign `ReferenceError`; sloppy assign recreates; `typeof` → `"undefined"`.
4. `global-property-accessor-after-warm.js` — getter/setter installed after warm: getter called once per read, setter once per write, `x++` calls getter then setter once each, correct receiver.
5. `global-lexical-shadows-global-property-after-warm.js` — configurable global property `z` warmed, then `$262.evalScript("let z = 5")` (§16.1.7): subsequent bare reads/writes hit the lexical binding, `globalThis.z` untouched; `const` assignment `TypeError`; TDZ read `ReferenceError` from a warmed function.
6. `global-eval-var-shadows-global-after-warm.js` — closure reads global `x`; enclosing function then runs sloppy direct `eval("var x = 1")`; closure sees the eval var (§19.2.1.3).
7. `global-object-prototype-proxy-after-warm.js` — own data property must not invoke traps; after `delete`, `Object.setPrototypeOf(globalThis, new Proxy({}, {has,get,set}))` traps fire for read/write/typeof (§9.1.1.2.6 steps, §10.5.7/§10.5.9).
8. `global-same-name-in-two-realms.js` — `$262.createRealm()`: same-named var in both realms; a function from realm B reads/writes B's global when called from A; compiled hot loop in each.
9. `global-with-object-shadows-global-var-after-warm.js` — `with ({x: 1}) { x; x = 2 }` after warm; `Symbol.unscopables`.
10. `global-var-assignment-gc-write-barrier.js` — assign a freshly allocated object/closure/string to a warmed global var each iteration, `$262.gc()`, read back (named `...-gc-rooting` conventions; also run under `JSSE_GC_STRESS=7`).
11. `global-update-expression-semantics.js` — `x++`/`--x` on number, string, BigInt, `undefined`, object with `valueOf`, non-writable, accessor; result values and call order.
12. `global-compound-assignment-rhs-mutates-binding.js` — `x = (delete globalThis.x, 1)` / `x = (Object.defineProperty(globalThis,"x",{writable:false}), 1)`: reference resolved before RHS, re-validated at PutValue (§6.2.5.6).

### 2. Non-cloning global reads (R1/R2)
- **Red**: whitebox tests in `src/interpreter/tests.rs`:
  `global_object_own_data` returns `Some(v)` for an ordinary own data property, `None` for
  accessor, missing, inherited, a Proxy global (`Object.setPrototypeOf` target excluded), module-namespace/array-like
  objects, and does **not** mutate the object.
- **Green**: create `src/interpreter/global_access.rs`:
  `pub(super) fn global_object_own_data(&self, gid: u64, name: &str) -> Option<JsValue>` — one
  `get_object_cell(gid)` + one `properties.get(name)`; reject proxy / revoked proxy / module namespace /
  mapped-arguments / typed-array / array-elements objects (same exotic predicates `get_own_property` and
  `own_property_lookup` consult); return the value only when `get.is_none() && set.is_none()`.
- Wire into `resolve_identifier` step 4 (`eval.rs:7822-7858`) and `resolve_global_getter`
  (`eval.rs:7880-7905`): try the helper first; on `None` fall through to the **unchanged** existing code.
  Also drop the duplicate first-scope scan in `vm.rs::load_name_value` (it re-checks the frame env that
  `resolve_identifier` immediately re-checks) by calling `resolve_identifier` directly when the first probe misses and the
  scope is not the one just probed (keep the `with`/indirect guards).
- Counters: `global_prop_fast_hits` on helper success.

### 3. Single-probe global stores (W1)
- **Red**: slice-0 counter test extended (stores must register fast hits, and `atom_*` counters stay flat);
  whitebox test: `global_object_store_own_data` returns `false` and leaves the property untouched for
  non-writable, accessor, missing, proxy, frozen.
- **Green**: `pub(super) fn global_object_store_own_data(&mut self, gid: u64, name: &str, value: &JsValue) -> bool`:
  one `get_object_cell`, one `properties.get_mut`; requires an existing own **data** descriptor with
  `writable != Some(false)` and the same exotic guards as slice 2; performs
  `gc_write_barrier_value(&cell, value)` **before** the write (identical to `env_set`'s `WriteBinding`
  and the `proxy_set` own-data branch), then `desc.value = Some(value.clone())`. Never creates a property.
- Wire into `put_value_by_ref`'s `IdentifierRef::SpecificEnv(specific_env)` arm at the top: when
  `specific_env.global_object_id` is `Some(gid)` **and** `specific_env.bindings` lacks `key` and has no
  indirect binding, try the helper; `true` → `return Completion::Normal(value)`. Otherwise run the existing
  `env_check_set_binding` → `proxy_has_property` → `proxy_set` → `sync_global_object_binding` sequence
  unchanged. (Names that are also in `global_env.bindings` — implicit globals, eval vars, function
  declarations — keep the old path, which also keeps `sync_global_object_binding`'s binding mirror correct.)
- Same wiring covers the VM (`Op::StoreResolvedName`'s fallthrough to `put_value_by_ref`) and every
  tree-walker assignment form (`eval.rs:2765-4073`), so `--no-bytecode` improves too.

### 4. Single read-modify-write for `x++`/`--x` (W2; do only if slice-5 profile still shows it)
- In `eval_identifier_update` after `resolve_identifier_ref` yields `SpecificEnv(G)` with the same
  guards as slice 3: when the current own data value is a Number, update in place through one `get_mut`
  (barrier not needed for numbers); otherwise unchanged generic path.
- Red: `global-update-expression-semantics.js` stays green; counter test asserts fast hits for `i++`.

### 5. Measure and gate
- Re-run slice-0 matrix. **Gate**: `globalvars` after/base ≤ 0.72 **and**
  `typedarray_global` ≤ 1.15 × `typedarray_local` (same session, interleaved). Full test262 + extras
  (commands below) green. If both hold → skip slice 6, go to slice 7.

### 6. (Likely needed for `typedarray_global`; decided by the slice-2 checkpoint) Verified global-property slot hints
- **Red**: `property_map.rs` unit tests: `slot_of`/`entry_at` round-trip for inline and spilled maps; a
  hint stays *safe* after `remove` (swap-remove moves an entry), after spill, and after reinsert — i.e.
  `entry_at(slot)` is trusted only when the stored key is `==` the probe key.
- **Green**: make `PropertyMap::Spilled` an insertion-ordered `Vec<(JsPropertyKey, PropertyDescriptor)>`
  plus a key→slot index (mirror of `BindingMap`), keeping `iter()`/`keys()` semantics (callers already use
  `property_order` for ordered iteration). Add `Chunk.global_hints: Vec<Cell<u32>>` (one per name pool entry,
  built in `compiler.rs` finish; hints verified on every use exactly like `LoadLocal`). `global_access.rs`
  helpers take `hint: &Cell<u32>`; hit = `entry_at(hint)` key-equal + data descriptor; miss re-probes and
  retrains. No invalidation protocol: a stale hint costs one failed key compare.
- Regression bullets for this slice: (a) generic `get`/`set`/`has`/`delete`/`keys` on every object
  that has spilled (all objects > 4 properties) — compare `benchmarks/scripts/bench_object.js`,
  `bench_opmix.js`, `bench_json.js`, and a startup run (`jsse -e 1`, builtins are spilled maps) before/after;
  (b) `js_object_data_size_under_3kb` tripwire (`property_map.rs` tests) must stay green; (c) removal order:
  `swap_remove` must fix the moved entry's index and never reorder `property_order`; (d) GC trace of
  property values (`gc::trace_object_fields`) iterates the new storage; (e) full test262 + `--no-bytecode`.

### 7. Docs and cleanup
- ADR `docs/adr/2026-10-12-1500-global-object-data-fast-path.md`; `CONTEXT.md` term **Global Data Fast Path**
  (avoid: global IC, property cell); `AGENTS.md`: `global_access.rs` in Source Layout + the two counters;
  `docs/perf/2026-10-12/905-global-reads.md` with tables and callgrind deltas.

## Test surface

- Targeted test262 (run before the full suite):
  `language/global-code`, `language/eval-code`, `language/identifier-resolution`,
  `language/expressions/{assignment,compound-assignment,postfix-increment,prefix-increment,postfix-decrement,prefix-decrement,typeof,delete,call}`,
  `language/statements/{with,variable,let,const,class,function}`, `language/function-code`,
  `annexB/language/{global-code,eval-code}`, `annexB/built-ins`, `built-ins/global`,
  `built-ins/Object/{defineProperty,defineProperties,freeze,seal,preventExtensions,setPrototypeOf}`,
  `built-ins/{Reflect,eval,Function,ShadowRealm}`, `language/expressions/dynamic-import` (realm paths).
  `uv run python scripts/run-test262.py test262/test/<dir>/`.
- Full: `uv run python scripts/run-test262.py` (baseline read from `origin/main:test262-pass.txt`; no
  `--update-baseline`). `uv run python scripts/run-custom-tests.py`, `cargo test --release`,
  `cargo test --release --features perf-counters --test perf_counters_global_access`,
  `./scripts/lint.sh`.
- Extras: `uv run python scripts/run-test262.py test262-extra/` plain and `--no-bytecode`;
  `JSSE_GC_STRESS=7 ... test262-extra/` (touches GC write barrier); `cargo build --profile release-checked`
  + `--binary target/release-checked/jsse` on test262-extra and a 10% sample.
- Not covered by test262, therefore new under `test262-extra/` (slice 1 list): warm-then-mutate cases for
  accessor/non-writable/delete/lexical-shadow/eval-var/proxy-prototype/multi-realm/with/GC barrier/update
  semantics. `tests/` gets only the perf-counter test (engine-resource check, not an ECMAScript value).
- Library harness smoke (global-heavy bundles): `./scripts/run-library-tests.sh decimal.js` and
  `acorn` (both green today).

## Regression risk

- **`test262-pass.txt` movers**: `language/global-code` (decl instantiation order, non-configurable
  property conflicts), `language/eval-code/{direct,indirect}` var hoisting onto the global object,
  `built-ins/Object/defineProperty` 15.2.3.6-4-* global-object cases, `annexB/.../global-code`
  function-in-block hoisting, `built-ins/ShadowRealm` cross-realm global access.
- **Shared machinery**: tree-walker hot paths `resolve_identifier`/`put_value_by_ref`
  (`eval_expr` Identifier arm, every assignment form); the bytecode VM name ops; the property MOP
  (`proxy_set` own-data branch must stay observably identical); GC write barrier
  (`gc_write_barrier_value` — a missed barrier on an old global object storing a young value is a
  use-after-free under minor GC; hence the GC-stress runs); `gc_temp_roots` unaffected (no new roots —
  helpers return `JsValue` clones, no stored object ids in phase 1).
- **Exact-equivalence conditions** (the helpers must refuse, never approximate): Proxy / revoked
  proxy global object, accessor or non-writable property, name present in `global_env.bindings`
  (declarative record wins, TDZ, `ImmutableValue` for `NaN`/`Infinity`/`undefined`), `with` scopes
  (resolved earlier, unchanged), module namespace, indirect (import) bindings.
- **Slice 6 risk**: `PropertyMap` is under every object; a second indirection can slow
  generic property access and move `JsObjectData` size; guarded by the size tripwire and
  `bench_object`/`bench_opmix` before/after.
- **Host noise**: acceptance judged on ratios, never on `perf-counters` builds.

## Out of scope

- Skipping the intermediate scope walk (static hop count) and any "dynamic scope seen" poison flag.
- Making `shape_id` reliable for all mutators; the property-IC (#71) audit.
- Faster/keyed hasher for `PropertyMap` (W2-style) — separate issue.
- Caching builtin-method/`Math.floor` call sites beyond what falls out of the cheaper read.
- Compiling more constructs, `StoreNamePop` fusion (W4), dense Array paths (W6).
- Changing `typeof`/`delete` identifier semantics, module-binding resolution, or global declaration
  instantiation; any `--update-baseline` of `test262-pass.txt`.
