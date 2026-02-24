# Cross-Realm Support Implementation Plan

**Estimated impact: ~364 new test262 passes**
**Status: MERGED** — Realm struct, $262.createRealm/evalScript/global, GetFunctionRealm, cross-realm GetPrototypeFromConstructor. +156 net new passes (88,004→88,160, 95.84%). Proxy 79%→89%, TypedArrayConstructors 87%→89%, annexB 90%→92%.

## Summary

Implement `$262.createRealm()` which creates a new execution realm with its own set of intrinsics. Tests create objects in one realm and use them in another to verify built-in methods work across realm boundaries.

## Current Architecture

- The `Interpreter` struct holds ~55+ prototype fields directly (object_prototype, array_prototype, function_prototype, etc.)
- `setup_globals()` (~2,900 lines in `builtins/mod.rs`) creates all built-ins in a single `global_env`
- `create_object()` uses `self.object_prototype` as the default prototype
- `create_function()` uses `self.function_prototype` (or generator/async variants)
- `JsFunction` does not carry a realm reference
- `JsObjectData` has no realm field
- `get_prototype_from_new_target()` does not implement `GetFunctionRealm`
- No concept of "realm" exists yet

## Architectural Approach: Realm Struct Extracted from Interpreter

Create a `Realm` struct that holds all intrinsic prototypes, the global environment, and the global object. The `Interpreter` retains shared state (object store, GC, symbol registry) and holds a list of `Realm` instances. A `current_realm_id` tracks which realm is active.

## Phase 1: Define the Realm Struct

**File: `src/interpreter/types.rs`**

1. Define a `Realm` struct containing:
   - All prototype fields currently on `Interpreter` (~55+ fields)
   - `global_env: EnvRef`
   - `global_object: Option<Rc<RefCell<JsObjectData>>>`
   - `throw_type_error: Option<JsValue>`
   - `realm_id: u64`

2. Add `realm_id: u64` field to `JsObjectData`

3. Add `realm_id: Option<u64>` field to `JsFunction::User` and `JsFunction::Native` variants (implementing `[[Realm]]` internal slot)

**Sub-tasks:**
- [ ] 1a. Create `Realm` struct with all prototype fields moved from Interpreter
- [ ] 1b. Add `realm_id` to `JsObjectData`
- [ ] 1c. Add `realm_id` to `JsFunction` variants
- [ ] 1d. Add `next_realm_id: u64` counter to Interpreter

## Phase 2: Refactor Interpreter to Use Realm

**File: `src/interpreter/mod.rs`**

1. Replace individual prototype fields on `Interpreter` with `realms: Vec<Realm>` and `current_realm_id: u64`
2. Add accessor methods: `current_realm()`, `current_realm_mut()`, `get_realm(id)`
3. Modify `create_object()` to use `self.current_realm().object_prototype`
4. Modify `create_function()` to use realm prototypes and set `realm_id` on new functions
5. Modify `Interpreter::new()` to create the initial realm (realm 0)
6. Replace `self.global_env` with `self.current_realm().global_env` everywhere

**Sub-tasks:**
- [ ] 2a. Add `realms` and `current_realm_id` to Interpreter, keep old fields temporarily as aliases
- [ ] 2b. Create accessor methods for current realm
- [ ] 2c. Move `global_env` access to go through realm
- [ ] 2d. Update `create_object()` to use realm prototype
- [ ] 2e. Update `create_function()` to use realm prototypes and set realm_id
- [ ] 2f. Remove old individual prototype fields once all accesses are migrated

## Phase 3: Refactor setup_globals into Realm Initialization

**File: `src/interpreter/builtins/mod.rs`**

1. Refactor `setup_globals()` to populate a `Realm` struct rather than setting fields on `Interpreter`
2. Ensure all `setup_*_prototype()` methods use `self.current_realm_mut()` for storing prototypes
3. Extract test harness setup ($262, Test262Error, print, console) from realm initialization
4. Create `create_new_realm(&mut self) -> u64` that switches to new realm, runs setup_globals, switches back

**Sub-tasks:**
- [ ] 3a. Modify setup_globals to write prototypes into the current realm
- [ ] 3b. Ensure all setup_*_prototype() methods use current_realm_mut()
- [ ] 3c. Extract test harness setup from realm initialization
- [ ] 3d. Create create_new_realm() method

## Phase 4: Implement GetFunctionRealm

**File: `src/interpreter/eval.rs`**

1. Implement `get_function_realm(&self, func_val: &JsValue) -> u64`:
   - If callable has `realm_id`, return it
   - Bound Function: recurse on `[[BoundTargetFunction]]`
   - Proxy: recurse on `[[ProxyTarget]]` (throw if revoked)
   - Fallback: return `self.current_realm_id`

2. Modify `get_prototype_from_new_target()` to implement full spec:
   - If `newTarget.prototype` is not an Object, call `get_function_realm(newTarget)` and use that realm's intrinsic

3. Create `realm_intrinsic(&self, realm_id: u64, name: &str)` name-to-prototype mapping

**Sub-tasks:**
- [ ] 4a. Implement `get_function_realm()` method
- [ ] 4b. Create `realm_intrinsic()` name-to-prototype mapping
- [ ] 4c. Modify `get_prototype_from_new_target()` to use GetFunctionRealm fallback
- [ ] 4d. Update all built-in constructors to pass intrinsic name

## Phase 5: Implement $262.createRealm

**File: `src/interpreter/builtins/mod.rs`**

1. Add `createRealm` method on `$262` object:
   - Save current_realm_id
   - Create new realm with new global env, run setup_globals
   - Create new $262 object in new realm with: global, createRealm, detachArrayBuffer, gc, evalScript
   - Restore current_realm_id
   - Return new $262 object

2. Add `evalScript` to `$262`: parse and execute source string in the $262's realm's global scope

3. Add `global` property to `$262`: reference to realm's global object

**Sub-tasks:**
- [ ] 5a. Add `global` property to existing $262 object
- [ ] 5b. Implement `create_new_realm()` in Interpreter
- [ ] 5c. Add `createRealm` native function to $262
- [ ] 5d. Add `evalScript` native function to $262
- [ ] 5e. Wire up $262 creation for new realms (recursive support)

## Phase 6: Fix eval Realm Semantics

**File: `src/interpreter/eval.rs`**

1. `is_builtin_eval()` must check that the function is the current realm's `eval`
2. `perform_eval()` for indirect eval must use the eval function's realm's global environment
3. Each realm's `eval` carries realm_id for correct indirect eval scope

**Sub-tasks:**
- [ ] 6a. Store built-in eval function's object ID per realm
- [ ] 6b. Modify `is_builtin_eval()` to compare against current realm's eval
- [ ] 6c. Modify `perform_eval()` to use eval function's realm for indirect eval
- [ ] 6d. Ensure another realm's eval used as `eval(...)` is NOT direct eval

## Phase 7: Fix Error Construction Realm

**Files: `src/interpreter/eval.rs`, `src/interpreter/helpers.rs`**

1. `create_type_error()` etc. should use the current realm's error prototype
2. For proxy/bound function errors, ensure error comes from correct realm
3. Add `create_error_in_realm(realm_id, kind, msg)` method

**Sub-tasks:**
- [ ] 7a. Ensure create_type_error() uses current realm's error prototype
- [ ] 7b. Fix proxy/bound function invocation errors for correct realm
- [ ] 7c. Add create_error_in_realm() method

## Phase 8: Update GC Root Set

**File: `src/interpreter/gc.rs`**

1. Add `collect_roots()` method to `Realm` struct
2. Modify `maybe_gc()` to iterate all realms for root collection

**Sub-tasks:**
- [ ] 8a. Add collect_roots() to Realm
- [ ] 8b. Modify maybe_gc() to iterate all realms

## Phase 9: Shared State Verification

1. GlobalSymbolRegistry: already on Interpreter, shared across realms (no change)
2. Object store: `self.objects` is per-interpreter, objects from different realms coexist
3. Well-known symbols: created once, stored in symbol registry (shared automatically)

**Sub-tasks:**
- [ ] 9a. Verify well-known symbols are shared
- [ ] 9b. Ensure global_symbol_registry stays on Interpreter

## Key Challenges and Risks

1. **Scale of refactoring**: Moving ~55 prototype fields and updating every reference across ~30,000+ lines
2. **Borrow checker friction**: Wrapping prototypes behind Realm accessor may cause borrow conflicts. Mitigation: Clone Rc before use.
3. **Performance**: Creating a full realm is expensive. Mitigation: tests only create 1-3 realms each.
4. **Circular references in setup**: setup_globals has complex ordering that must work for second realm too
5. **Incremental approach**: Introduce Realm struct first, verify no regressions, then add createRealm

## Key Test Files

- `test262/test/built-ins/Symbol/for/cross-realm.js` (shared symbol registry)
- `test262/test/built-ins/Array/proto-from-ctor-realm-zero.js` (GetPrototypeFromConstructor)
- `test262/test/language/eval-code/indirect/realm.js` (indirect eval in other realm)
- `test262/test/harness/assert-throws-same-realm.js` (cross-realm instanceof)
- `test262/test/built-ins/Proxy/get-fn-realm.js` (GetFunctionRealm through proxy)

## Critical Files

- `src/interpreter/types.rs` — Realm struct, realm_id on JsObjectData/JsFunction
- `src/interpreter/mod.rs` — Interpreter holds Vec<Realm>, accessor methods, create_object/create_function
- `src/interpreter/builtins/mod.rs` — setup_globals refactor, $262.createRealm/evalScript/global
- `src/interpreter/eval.rs` — GetFunctionRealm, get_prototype_from_new_target, perform_eval
- `src/interpreter/gc.rs` — Root collection across all realms
