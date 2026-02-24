# Cross-Realm Implementation Plan — Detailed Steps

**Estimated impact: ~279 test262 tests** (excluding 77 staging/sm tests)
**Current baseline: 87,842/91,986 (95.49%)**

## Architecture Overview

The core idea: extract all realm-specific state from `Interpreter` into a `Realm` struct. The `Interpreter` keeps shared state (object store, GC, symbol registry, microtask queue) and holds a `Vec<Realm>` plus `current_realm_id: usize`. All prototype access goes through the current realm.

### What lives in Realm (per-realm):
- All ~65 prototype fields (`object_prototype`, `array_prototype`, etc.)
- All constructor value fields (`typed_array_constructor`, `intl_*_ctor`)
- `global_env: EnvRef`
- `global_object: Option<Rc<RefCell<JsObjectData>>>`
- `throw_type_error: Option<JsValue>`
- `template_cache: HashMap<usize, u64>`

### What stays on Interpreter (shared):
- `objects: Vec<Option<Rc<RefCell<JsObjectData>>>>` — shared object store
- `global_symbol_registry` — shared per spec (Symbol.for works cross-realm)
- `next_symbol_id` — shared counter
- `new_target` — execution state
- `free_list`, `gc_alloc_count` — GC state
- `generator_context`, `destructuring_yield` — execution state
- `microtask_queue`, `microtask_roots` — shared task queue
- `cached_has_instance_key` — optimization cache
- `module_registry`, `current_module_path` — modules
- `call_stack_envs`, `gc_temp_roots` — execution/GC state
- `class_private_names`, `next_class_brand_id` — class state
- `regexp_legacy_*`, `regexp_constructor_id` — RegExp state
- `constructing_derived`, `last_call_*` — call state

---

## Phase 1: Define Realm Struct (Compile-only, no behavior change)

**Goal**: Define `Realm` with all prototype fields; add it to `Interpreter` alongside existing fields. No code paths use it yet.

### Step 1.1: Define `Realm` struct in `types.rs`

Add after the existing type definitions:

```rust
pub struct Realm {
    pub id: usize,
    pub global_env: EnvRef,
    pub global_object: Option<Rc<RefCell<JsObjectData>>>,
    pub throw_type_error: Option<JsValue>,
    pub template_cache: HashMap<usize, u64>,

    // All prototype fields (copy from Interpreter)
    pub object_prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    // ... (all 65+ fields)

    // Constructor values
    pub typed_array_constructor: Option<JsValue>,
    pub intl_number_format_ctor: Option<JsValue>,
    pub intl_date_time_format_ctor: Option<JsValue>,
    pub intl_duration_format_ctor: Option<JsValue>,
}
```

Add `Realm::new(id: usize, global_env: EnvRef) -> Self` constructor initializing all to `None`.

### Step 1.2: Add `realms` and `current_realm_id` to `Interpreter`

In `mod.rs`, add two new fields to `Interpreter`:
```rust
pub(crate) realms: Vec<Realm>,
pub(crate) current_realm_id: usize,
```

In `Interpreter::new()`, after creating `global_env`, create `Realm { id: 0, global_env: global.clone(), ... }` and push it. Set `current_realm_id = 0`.

### Step 1.3: Add accessor methods

```rust
pub(crate) fn current_realm(&self) -> &Realm { &self.realms[self.current_realm_id] }
pub(crate) fn current_realm_mut(&mut self) -> &mut Realm { &mut self.realms[self.current_realm_id] }
pub(crate) fn get_realm(&self, id: usize) -> &Realm { &self.realms[id] }
pub(crate) fn get_realm_mut(&mut self, id: usize) -> &mut Realm { &mut self.realms[id] }
```

**Verification**: `cargo build --release` compiles. Run test262 on a small subset to confirm no regressions.

---

## Phase 2: Migrate Prototype Writes (setup_globals)

**Goal**: `setup_globals()` writes prototypes into `self.realms[0]` instead of `self.<field>`. Old fields still exist as aliases.

### Step 2.1: Modify `setup_globals()` to write to current realm

Every line like:
```rust
self.object_prototype = Some(obj_proto.clone());
```
becomes:
```rust
self.realms[self.current_realm_id].object_prototype = Some(obj_proto.clone());
self.object_prototype = self.realms[self.current_realm_id].object_prototype.clone();
```

This is a mechanical transform across ~65 prototype assignments in `builtins/mod.rs`.

Similarly for `throw_type_error`, `typed_array_constructor`, `intl_*_ctor`.

Also store `global_object` in the realm:
```rust
self.realms[self.current_realm_id].global_object = Some(global_obj.clone());
```

And `template_cache` — initially just leave on Interpreter (it's an optimization that can stay shared for now).

### Step 2.2: Modify all `setup_*_prototype()` methods

These are in `builtins/array.rs`, `builtins/string.rs`, `builtins/number.rs`, `builtins/iterators.rs`, `builtins/collections.rs`, `builtins/date.rs`, `builtins/mod.rs`, etc.

Each writes prototypes like:
```rust
self.map_prototype = Some(map_proto.clone());
```
Change to write to realm:
```rust
self.realms[self.current_realm_id].map_prototype = Some(map_proto.clone());
self.map_prototype = self.realms[self.current_realm_id].map_prototype.clone();
```

**Files to modify** (each has prototype assignments):
- `builtins/mod.rs` (~42 occurrences)
- `builtins/iterators.rs` (~17)
- `builtins/collections.rs` (~10)
- `builtins/typedarray.rs` (~38)
- `builtins/promise.rs` (~4)
- `builtins/number.rs` (~3)
- `builtins/array.rs` (~3)
- `builtins/string.rs` (~1)
- `builtins/regexp.rs` (~1)
- `builtins/date.rs` (~1)
- `builtins/bigint.rs` (~1)
- `builtins/disposable.rs` (~2)
- `builtins/temporal/*.rs` (~8)
- `builtins/intl/*.rs` (~22)

### Step 2.3: Store global_env in realm

Change `Interpreter::new()` so the global environment is stored in both `self.global_env` and `self.realms[0].global_env`.

**Verification**: `cargo build --release` + full test262 — expect 0 regressions since old fields are still populated as aliases.

---

## Phase 3: Migrate Prototype Reads

**Goal**: All code that reads `self.object_prototype`, etc. now reads from `self.current_realm().<field>`. Remove old fields from Interpreter.

### Step 3.1: Migrate reads in `eval.rs` (~31 occurrences)

Systematically replace:
```rust
self.object_prototype.clone()     →  self.current_realm().object_prototype.clone()
self.array_prototype.clone()      →  self.current_realm().array_prototype.clone()
self.function_prototype.clone()   →  self.current_realm().function_prototype.clone()
// etc.
```

Key methods affected:
- `eval_new()` — uses prototypes for constructor results
- `eval_object_literal()` — uses `object_prototype`
- `eval_array_literal()` — uses `array_prototype`
- `eval_regex_literal()` — uses `regexp_prototype`
- Various built-in method calls that create result objects

### Step 3.2: Migrate reads in `mod.rs` (~11 occurrences)

- `create_object()` — `self.object_prototype` → `self.current_realm().object_prototype`
- `create_function()` — `self.function_prototype`, `self.generator_function_prototype`, etc.
- `get_prototype_from_new_target()` — default prototype parameter
- Other helper methods

### Step 3.3: Migrate reads in `builtins/*.rs`

These read prototypes when constructing new instances:
- `builtins/typedarray.rs` (~38) — typed array constructor reads type-specific prototypes
- `builtins/iterators.rs` (~17) — iterator creation reads iterator prototypes
- `builtins/collections.rs` (~10) — Map/Set/WeakMap/WeakSet constructors
- All other builtin files

### Step 3.4: Migrate reads in `gc.rs` (~62 occurrences)

Replace the big array of prototype refs in `maybe_gc()`:
```rust
// OLD: for proto in [&self.object_prototype, &self.array_prototype, ...]
// NEW:
for realm in &self.realms {
    realm.collect_roots(&mut worklist);
}
```

Add `Realm::collect_roots(&self, worklist: &mut Vec<u64>)` that iterates all its prototypes and constructors.

Also migrate `throw_type_error` root to realm iteration, and `template_cache`.

### Step 3.5: Migrate `global_env` reads

Search for all `self.global_env` usages. Replace with `self.current_realm().global_env`:
- `perform_eval()` indirect eval path
- `register_global_fn()`
- `setup_globals()` binding declarations
- `run()` entry point
- Various places that look up globals

This is the highest-risk step. `global_env` is used extensively. Count occurrences first:

### Step 3.6: Remove old fields from Interpreter

Once all reads are migrated, delete the ~65 prototype fields, `throw_type_error`, and `global_env` from the `Interpreter` struct. Also remove them from `Interpreter::new()`.

**Verification**: `cargo build --release` + full test262 — expect 0 regressions.

---

## Phase 4: Add `realm_id` to Functions

**Goal**: Each function knows which realm it was created in.

### Step 4.1: Add `realm_id` to `JsFunction`

```rust
pub enum JsFunction {
    User {
        // existing fields...
        realm_id: usize,   // NEW
    },
    Native(String, usize, Rc<dyn Fn(...)>, bool, usize),  // add realm_id as 5th field
}
```

Update all pattern matches on `JsFunction` across the codebase (search for `JsFunction::User {` and `JsFunction::Native(`).

### Step 4.2: Set `realm_id` in `create_function()`

```rust
fn create_function(&mut self, func: JsFunction) -> JsValue {
    // Set realm_id to current_realm_id on the function
    // ...
}
```

Also update `JsFunction::native()` constructor to accept realm_id.

### Step 4.3: Implement `get_function_realm()`

```rust
pub(crate) fn get_function_realm(&self, func_val: &JsValue) -> usize {
    if let JsValue::Object(o) = func_val
        && let Some(obj) = self.get_object(o.id)
    {
        let obj = obj.borrow();
        // Bound function: recurse on [[BoundTargetFunction]]
        if let Some(ref target) = obj.bound_target_function {
            return self.get_function_realm(target);
        }
        // Proxy: recurse on [[ProxyTarget]]
        if let Some(ref target) = obj.proxy_target
            && !obj.proxy_revoked
        {
            let target_id = target.borrow().id.unwrap();
            return self.get_function_realm(&JsValue::Object(JsObject { id: target_id }));
        }
        // Regular function with realm_id
        if let Some(ref func) = obj.callable {
            match func {
                JsFunction::User { realm_id, .. } => return *realm_id,
                JsFunction::Native(_, _, _, _, realm_id) => return *realm_id,
            }
        }
    }
    self.current_realm_id
}
```

**Verification**: `cargo build --release` + test262 subset — expect 0 regressions (realm_id exists but isn't used for dispatch yet).

---

## Phase 5: Implement GetPrototypeFromConstructor with Realm Fallback

**Goal**: When `new.target.prototype` is not an object, fall back to the intrinsic from the new target's realm, not the current realm.

### Step 5.1: Create intrinsic name mapping

```rust
pub(crate) fn realm_intrinsic_proto(&self, realm_id: usize, name: &str) -> Option<Rc<RefCell<JsObjectData>>> {
    let realm = &self.realms[realm_id];
    match name {
        "%Object.prototype%" => realm.object_prototype.clone(),
        "%Array.prototype%" => realm.array_prototype.clone(),
        "%Function.prototype%" => realm.function_prototype.clone(),
        // ... all intrinsics
        _ => None,
    }
}
```

### Step 5.2: Update `get_prototype_from_new_target()`

Change signature to accept an intrinsic name:
```rust
pub(crate) fn get_prototype_from_new_target(
    &mut self,
    default_proto: &Option<Rc<RefCell<JsObjectData>>>,
    intrinsic_name: &str,  // NEW
) -> Result<Option<Rc<RefCell<JsObjectData>>>, JsValue>
```

When `new.target.prototype` is not an object:
```rust
// OLD: return Ok(default_proto.clone())
// NEW:
let realm_id = self.get_function_realm(&nt);
return Ok(self.realm_intrinsic_proto(realm_id, intrinsic_name));
```

### Step 5.3: Update all callers

Every call site passes the appropriate intrinsic name. There are ~30+ call sites across:
- `builtins/mod.rs` (Object, Error constructors)
- `builtins/array.rs` (Array constructor)
- `builtins/collections.rs` (Map, Set, WeakMap, WeakSet, WeakRef, FinalizationRegistry)
- `builtins/typedarray.rs` (TypedArray constructors)
- `builtins/promise.rs` (Promise constructor)
- `builtins/date.rs` (Date constructor)
- `builtins/regexp.rs` (RegExp constructor)
- `eval.rs` (various constructors)

**Verification**: test262 on `built-ins/Array/proto-from-ctor-realm*` and similar.

---

## Phase 6: Implement `$262.createRealm()`

**Goal**: The test262 harness can call `$262.createRealm()` to get a new realm with its own globals.

### Step 6.1: Add `$262.global` property

In `setup_globals()`, after creating the `$262` object, add:
```rust
let global_obj_val = JsValue::Object(JsObject { id: global_object_id });
dollar_262.borrow_mut().insert_builtin("global".to_string(), global_obj_val);
```

### Step 6.2: Implement `create_new_realm()` on Interpreter

```rust
pub(crate) fn create_new_realm(&mut self) -> usize {
    let new_id = self.realms.len();
    let new_global_env = Environment::new(None);
    // Initialize basic global constants
    {
        let mut env = new_global_env.borrow_mut();
        for (name, value) in [
            ("undefined", JsValue::Undefined),
            ("NaN", JsValue::Number(f64::NAN)),
            ("Infinity", JsValue::Number(f64::INFINITY)),
        ] {
            env.bindings.insert(name.to_string(), Binding {
                value, kind: BindingKind::Const,
                initialized: true, deletable: false,
            });
        }
    }
    let realm = Realm::new(new_id, new_global_env);
    self.realms.push(realm);

    // Switch to new realm, run setup_globals, switch back
    let old_realm = self.current_realm_id;
    self.current_realm_id = new_id;
    self.setup_realm_globals();  // new method: setup_globals minus test harness
    self.current_realm_id = old_realm;
    new_id
}
```

### Step 6.3: Split `setup_globals()` into realm-init + harness-init

Extract the core built-in setup (Object, Array, String, Number, etc.) into `setup_realm_globals()`. Keep test harness ($262, console, print) in a separate `setup_test_harness()`.

Current `setup_globals()` becomes:
```rust
pub(crate) fn setup_globals(&mut self) {
    self.setup_realm_globals();
    self.setup_test_harness();
}
```

### Step 6.4: Add `$262.createRealm` native function

```rust
let create_realm_fn = self.create_function(JsFunction::native(
    "createRealm".to_string(),
    0,
    |interp, _this, _args| {
        let new_realm_id = interp.create_new_realm();
        // Create $262 for the new realm
        let old_realm = interp.current_realm_id;
        interp.current_realm_id = new_realm_id;
        let new_dollar_262 = interp.create_realm_dollar_262(new_realm_id);
        interp.current_realm_id = old_realm;
        Completion::Normal(new_dollar_262)
    },
));
```

### Step 6.5: Add `$262.evalScript` native function

```rust
// evalScript(code): parse and eval `code` in this $262's realm's global scope
let eval_script_fn = self.create_function(JsFunction::native(
    "evalScript".to_string(),
    1,
    |interp, this, args| {
        // Get the realm_id from this $262 object's __realm_id__ internal property
        let realm_id = /* extract from this */;
        let code = to_js_string(&args[0]);
        let old_realm = interp.current_realm_id;
        interp.current_realm_id = realm_id;
        let global_env = interp.current_realm().global_env.clone();
        // Parse and execute in that realm
        let result = /* parse and exec */;
        interp.current_realm_id = old_realm;
        result
    },
));
```

### Step 6.6: Create `create_realm_dollar_262()` helper

Builds a `$262` object for a given realm with: `global`, `createRealm`, `evalScript`, `detachArrayBuffer`, `gc`.

**Verification**: Run test262 on:
- `test262/test/harness/assert-throws-same-realm.js`
- `test262/test/built-ins/Symbol/for/cross-realm.js`
- `test262/test/built-ins/Array/proto-from-ctor-realm*.js`

---

## Phase 7: Fix Eval Realm Semantics

**Goal**: Indirect eval uses the eval function's realm's global environment.

### Step 7.1: Track built-in eval object ID per realm

In `Realm`, add:
```rust
pub builtin_eval_id: Option<u64>,
```

Set this when creating the `eval` function in `setup_realm_globals()`.

### Step 7.2: Fix `is_builtin_eval()`

```rust
fn is_builtin_eval(&self, val: &JsValue) -> bool {
    if let JsValue::Object(o) = val {
        // Must be the CURRENT realm's eval
        if let Some(eval_id) = self.current_realm().builtin_eval_id {
            return o.id == eval_id;
        }
    }
    false
}
```

### Step 7.3: Fix `perform_eval()` for indirect eval

For indirect eval, use the eval function's realm's global env instead of `self.global_env`:
```rust
let base = if direct {
    caller_env.clone()
} else {
    // Get the realm of the eval function being called
    let eval_realm_id = self.get_function_realm(&eval_func_val);
    self.realms[eval_realm_id].global_env.clone()
};
```

This requires threading the eval function value through to `perform_eval()`.

**Verification**: `test262/test/language/eval-code/indirect/realm.js`

---

## Phase 8: Fix Error Construction Realm

### Step 8.1: Error prototypes from current realm

Ensure `create_error()`, `create_type_error()`, etc. use `self.current_realm()` prototypes.

### Step 8.2: Add `create_error_in_realm(realm_id, kind, msg)`

For cases where errors must be created in a specific realm (e.g., proxy handler errors).

**Verification**: `test262/test/built-ins/Proxy/get-fn-realm.js`

---

## Phase 9: Update GC for Multi-Realm

### Step 9.1: Add `Realm::collect_roots()`

```rust
impl Realm {
    pub fn collect_roots(&self, worklist: &mut Vec<u64>) {
        Interpreter::collect_env_roots(&self.global_env, worklist);
        for proto in [
            &self.object_prototype,
            &self.array_prototype,
            // ... all prototypes
        ] {
            if let Some(p) = proto && let Some(id) = p.borrow().id {
                worklist.push(id);
            }
        }
        // constructors, throw_type_error, etc.
    }
}
```

### Step 9.2: Update `maybe_gc()` to iterate all realms

Replace the big prototype array with:
```rust
for realm in &self.realms {
    realm.collect_roots(&mut worklist);
}
```

---

## Execution Order and Risk Mitigation

### Recommended order:
1. **Phase 1** (Realm struct definition) — low risk, additive only
2. **Phase 2** (write migration) — medium risk, dual-write to old+new
3. **Phase 9** (GC update) — do alongside Phase 3
4. **Phase 3** (read migration + remove old fields) — **highest risk**, most code changes (~255 occurrences across 33 files)
5. **Phase 4** (realm_id on functions) — medium risk, many pattern matches to update
6. **Phase 5** (GetPrototypeFromConstructor) — medium risk
7. **Phase 6** ($262.createRealm) — medium risk, new functionality
8. **Phase 7** (eval semantics) — low risk, isolated changes
9. **Phase 8** (error realm) — low risk, isolated changes

### Checkpoints:
- After Phase 1: `cargo build --release` passes
- After Phase 2: full test262 — 0 regressions (dual-write)
- After Phase 3: full test262 — 0 regressions (migration complete)
- After Phase 4: full test262 — 0 regressions (realm_id exists)
- After Phase 5: test262 on `proto-from-ctor-realm` tests — new passes
- After Phase 6: test262 on `createRealm` tests — ~200+ new passes
- After Phase 7-8: remaining cross-realm tests pass

### Key risks:
1. **Borrow checker friction**: Accessing `self.current_realm().object_prototype` borrows `self` immutably, but many call sites also need `&mut self`. Mitigation: clone the `Rc` before using it.
2. **Phase 3 scale**: 255 prototype reads across 33 files is a massive find-and-replace. Mitigation: do it per-field, compile after each, use search-and-replace.
3. **setup_globals reentrancy**: `setup_globals()` calls methods that themselves access prototypes. When creating a second realm, the interpreter's "current realm" must be correctly set before calling these. Mitigation: save/restore `current_realm_id`.
4. **Template cache**: Currently per-interpreter. If moved to per-realm, tagged templates in different realms will correctly produce different frozen arrays. Low priority.

### File change estimates:
| File | Estimated changes |
|------|------------------|
| `types.rs` | +80 lines (Realm struct) |
| `mod.rs` | ~50 changes (accessor methods, prototype reads) |
| `eval.rs` | ~35 changes (prototype reads, eval semantics) |
| `gc.rs` | ~70 lines rewritten (realm iteration) |
| `builtins/mod.rs` | ~80 changes (setup_globals split, $262) |
| `builtins/typedarray.rs` | ~38 changes |
| `builtins/iterators.rs` | ~17 changes |
| `builtins/collections.rs` | ~10 changes |
| `builtins/intl/*.rs` | ~22 changes |
| `builtins/temporal/*.rs` | ~8 changes |
| Other builtin files | ~20 changes |
| **Total** | ~430 individual edits |
