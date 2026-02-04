# Symbol.species Implementation Plan

**Goal:** Implement `Symbol.species` accessor on built-in constructors to unlock ~25 direct tests and improve spec compliance for array/promise methods that create new instances.

**Current State:** 31,195 / 48,257 tests passing (64.64%)

---

## Overview

`Symbol.species` is a well-known symbol used to specify the constructor function used to create derived objects. When a method like `Array.prototype.map()` creates a new array, it consults `this.constructor[Symbol.species]` to determine what constructor to use.

**Spec Reference:** ECMA-262 §10.4.2.2 (ArraySpeciesCreate), §23.2.3.x (get Array [@@species])

---

## Spec Algorithm

For all built-ins, `[Symbol.species]` is an accessor property with:
- A getter that returns `this`
- No setter
- `configurable: true`, `enumerable: false`

```javascript
// Equivalent behavior:
Object.defineProperty(Array, Symbol.species, {
  get: function() { return this; },
  configurable: true,
  enumerable: false
});
```

---

## Constructors Requiring Symbol.species

| Constructor | Test Directory | Direct Tests | Status |
|-------------|----------------|--------------|--------|
| Array | `built-ins/Array/Symbol.species/` | 4 | Missing |
| ArrayBuffer | `built-ins/ArrayBuffer/Symbol.species/` | 4 | Missing |
| Map | `built-ins/Map/Symbol.species/` | 4 | Missing |
| Set | `built-ins/Set/Symbol.species/` | 4 | Missing |
| Promise | `built-ins/Promise/Symbol.species/` | 5 | Missing |
| RegExp | `built-ins/RegExp/Symbol.species/` | 4 | Missing |
| %TypedArray% | `built-ins/TypedArray/Symbol.species/` | 4 | Missing |

**Total direct tests:** 29 (currently 4 passing, 25 failing)

---

## Implementation

### Phase 1: Add Symbol.species Getters

Each constructor needs a `[Symbol.species]` accessor added in its setup function.

#### Pattern (reusable helper):

```rust
fn add_species_accessor(&mut self, constructor: &JsValue) {
    if let JsValue::Object(ref ctor_ref) = constructor
        && let Some(ctor_obj) = self.get_object(ctor_ref.id)
    {
        let species_key = self.get_symbol_key("species").unwrap_or_default();
        let getter = self.create_function(JsFunction::native(
            "get [Symbol.species]".to_string(),
            0,
            |_interp, this_val, _args| {
                Completion::Normal(this_val.clone())
            },
        ));
        ctor_obj.borrow_mut().insert_property(
            species_key,
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );
    }
}
```

#### Files to Modify:

1. **`src/interpreter/builtins/mod.rs`**
   - Add helper function `add_species_accessor()`
   - Call for: Object, Function (if applicable)

2. **`src/interpreter/builtins/array.rs`**
   - Add `[Symbol.species]` to Array constructor after setup

3. **`src/interpreter/builtins/collections.rs`**
   - Add `[Symbol.species]` to Map and Set constructors

4. **`src/interpreter/builtins/typedarray.rs`**
   - Add `[Symbol.species]` to %TypedArray% constructor
   - Add `[Symbol.species]` to ArrayBuffer constructor

5. **`src/interpreter/builtins/mod.rs`** (Promise section)
   - Add `[Symbol.species]` to Promise constructor

6. **`src/interpreter/builtins/mod.rs`** (RegExp section)
   - Add `[Symbol.species]` to RegExp constructor

---

## Phase 2: Use Species in Methods (Future Work)

Methods that should use `Symbol.species` for constructor selection:

### Array Methods:
- `Array.prototype.concat()`
- `Array.prototype.filter()`
- `Array.prototype.flat()`
- `Array.prototype.flatMap()`
- `Array.prototype.map()`
- `Array.prototype.slice()`
- `Array.prototype.splice()`

### Promise Methods:
- `Promise.prototype.then()`
- `Promise.prototype.catch()`
- `Promise.prototype.finally()`
- `Promise.all()`
- `Promise.allSettled()`
- `Promise.any()`
- `Promise.race()`

### TypedArray Methods:
- Similar array methods on TypedArray.prototype

### RegExp Methods:
- `RegExp.prototype[@@split]()`

**Note:** Phase 2 is more complex and can be done incrementally after Phase 1.

---

## Test Cases (29 direct tests)

| Category | Test | Description |
|----------|------|-------------|
| Array | `return-value.js` | Getter returns `this` |
| Array | `symbol-species.js` | Property exists as accessor |
| Array | `symbol-species-name.js` | Getter name is "get [Symbol.species]" |
| Array | `length.js` | Getter.length is 0 |
| (Similar for ArrayBuffer, Map, Set, Promise, RegExp, TypedArray) |

---

## Verification

```bash
# Build
cargo build --release

# Run Symbol.species tests
uv run python scripts/run-test262.py test262/test/built-ins/Array/Symbol.species/
uv run python scripts/run-test262.py test262/test/built-ins/ArrayBuffer/Symbol.species/
uv run python scripts/run-test262.py test262/test/built-ins/Map/Symbol.species/
uv run python scripts/run-test262.py test262/test/built-ins/Set/Symbol.species/
uv run python scripts/run-test262.py test262/test/built-ins/Promise/Symbol.species/
uv run python scripts/run-test262.py test262/test/built-ins/RegExp/Symbol.species/
uv run python scripts/run-test262.py test262/test/built-ins/TypedArray/Symbol.species/

# Run full suite to check for cascading improvements
uv run python scripts/run-test262.py
```

**Expected:** 25+ new passes from direct Symbol.species tests, potential additional passes from methods that already check species.

---

## Implementation Order

1. Create `add_species_accessor()` helper in `mod.rs`
2. Add to Array constructor
3. Add to Map and Set constructors
4. Add to ArrayBuffer constructor
5. Add to %TypedArray% constructor
6. Add to Promise constructor
7. Add to RegExp constructor
8. Run tests, verify no regressions
9. Update README.md and PLAN.md

---

## Risks and Considerations

- **Symbol key handling:** Must use symbol-aware property key for `Symbol.species`
- **Getter invocation:** Tests check that accessing `Constructor[Symbol.species]` invokes the getter with correct `this`
- **Property descriptor:** Must be non-enumerable, configurable, accessor (not data property)

---

## Files Summary

| File | Changes |
|------|---------|
| `src/interpreter/builtins/mod.rs` | Add helper, Promise/RegExp species |
| `src/interpreter/builtins/array.rs` | Array species |
| `src/interpreter/builtins/collections.rs` | Map/Set species |
| `src/interpreter/builtins/typedarray.rs` | ArrayBuffer/%TypedArray% species |
| `README.md` | Update test count |
| `PLAN.md` | Add implementation entry |
