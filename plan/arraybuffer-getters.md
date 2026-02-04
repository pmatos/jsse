# ArrayBuffer.prototype Getters Implementation Plan

**Goal:** Implement missing `ArrayBuffer.prototype` getters (`detached`, `resizable`, `maxByteLength`) to unlock ~30 direct tests and improve DataView/TypedArray test coverage.

**Current State:** 31,222 / 48,257 tests passing (64.70%)

---

## Overview

ArrayBuffer has several accessor properties that are currently missing:

| Property | Expected Value (non-resizable) | Direct Tests |
|----------|-------------------------------|--------------|
| `detached` | `false` (always, until detach is implemented) | 11 |
| `resizable` | `false` (always, until resizable buffers implemented) | 10 |
| `maxByteLength` | Same as `byteLength` (for non-resizable) | 11 |

**Spec References:**
- `detached`: ECMA-262 §25.1.5.1 (get ArrayBuffer.prototype.detached)
- `resizable`: ECMA-262 §25.1.5.4 (get ArrayBuffer.prototype.resizable)
- `maxByteLength`: ECMA-262 §25.1.5.2 (get ArrayBuffer.prototype.maxByteLength)

---

## Current Behavior

```javascript
var ab = new ArrayBuffer(8);
ab.byteLength    // 8 ✓
ab.detached      // undefined ✗ (should be false)
ab.resizable     // undefined ✗ (should be false)
ab.maxByteLength // undefined ✗ (should be 8)
```

---

## Spec Algorithms

### get ArrayBuffer.prototype.detached
1. Let O be the this value.
2. Perform ? RequireInternalSlot(O, [[ArrayBufferData]]).
3. If IsSharedArrayBuffer(O) is true, throw a TypeError exception.
4. Return IsDetachedBuffer(O).

For our implementation: Always return `false` (we don't support detaching yet).

### get ArrayBuffer.prototype.resizable
1. Let O be the this value.
2. Perform ? RequireInternalSlot(O, [[ArrayBufferData]]).
3. If IsSharedArrayBuffer(O) is true, throw a TypeError exception.
4. If IsFixedLengthArrayBuffer(O) is true, return false.
5. Return true.

For our implementation: Always return `false` (we only have fixed-length buffers).

### get ArrayBuffer.prototype.maxByteLength
1. Let O be the this value.
2. Perform ? RequireInternalSlot(O, [[ArrayBufferData]]).
3. If IsSharedArrayBuffer(O) is true, throw a TypeError exception.
4. If IsDetachedBuffer(O) is true, return +0.
5. If IsFixedLengthArrayBuffer(O) is true, return O.[[ArrayBufferByteLength]].
6. Return O.[[ArrayBufferMaxByteLength]].

For our implementation: Return `byteLength` (same as fixed-length behavior).

---

## Implementation

### File to Modify

`/home/pmatos/dev/jsse/src/interpreter/builtins/typedarray.rs`

Add after the `byteLength` getter setup in `setup_arraybuffer_prototype()`.

### Implementation Code

```rust
// ArrayBuffer.prototype.detached getter
let detached_getter = self.create_function(JsFunction::native(
    "get detached".to_string(),
    0,
    |interp, this_val, _args| {
        if let JsValue::Object(o) = this_val
            && let Some(obj) = interp.get_object(o.id)
        {
            if obj.borrow().arraybuffer_data.is_some() {
                // We don't support detaching, so always return false
                return Completion::Normal(JsValue::Boolean(false));
            }
        }
        Completion::Throw(interp.create_type_error(
            "ArrayBuffer.prototype.detached requires an ArrayBuffer"
        ))
    },
));
ab_proto.borrow_mut().insert_property(
    "detached".to_string(),
    PropertyDescriptor {
        value: None,
        writable: None,
        get: Some(detached_getter),
        set: None,
        enumerable: Some(false),
        configurable: Some(true),
    },
);

// ArrayBuffer.prototype.resizable getter
let resizable_getter = self.create_function(JsFunction::native(
    "get resizable".to_string(),
    0,
    |interp, this_val, _args| {
        if let JsValue::Object(o) = this_val
            && let Some(obj) = interp.get_object(o.id)
        {
            if obj.borrow().arraybuffer_data.is_some() {
                // We only support fixed-length buffers
                return Completion::Normal(JsValue::Boolean(false));
            }
        }
        Completion::Throw(interp.create_type_error(
            "ArrayBuffer.prototype.resizable requires an ArrayBuffer"
        ))
    },
));
ab_proto.borrow_mut().insert_property(
    "resizable".to_string(),
    PropertyDescriptor {
        value: None,
        writable: None,
        get: Some(resizable_getter),
        set: None,
        enumerable: Some(false),
        configurable: Some(true),
    },
);

// ArrayBuffer.prototype.maxByteLength getter
let max_byte_length_getter = self.create_function(JsFunction::native(
    "get maxByteLength".to_string(),
    0,
    |interp, this_val, _args| {
        if let JsValue::Object(o) = this_val
            && let Some(obj) = interp.get_object(o.id)
        {
            if let Some(ref buf) = obj.borrow().arraybuffer_data {
                // For fixed-length buffers, maxByteLength == byteLength
                return Completion::Normal(JsValue::Number(buf.borrow().len() as f64));
            }
        }
        Completion::Throw(interp.create_type_error(
            "ArrayBuffer.prototype.maxByteLength requires an ArrayBuffer"
        ))
    },
));
ab_proto.borrow_mut().insert_property(
    "maxByteLength".to_string(),
    PropertyDescriptor {
        value: None,
        writable: None,
        get: Some(max_byte_length_getter),
        set: None,
        enumerable: Some(false),
        configurable: Some(true),
    },
);
```

---

## Test Cases (32 direct tests)

### detached (11 tests)
- `detached-buffer.js` - requires actual detach support
- `invoked-as-accessor.js` - getter behavior
- `length.js` - getter.length === 0
- `name.js` - getter.name === "get detached"
- `prop-desc.js` - property descriptor checks
- `this-is-not-object.js` - TypeError for primitives
- etc.

### resizable (10 tests)
- Similar pattern to detached tests

### maxByteLength (11 tests)
- Similar pattern, plus value checks

---

## Expected Results

Some tests may still fail because they require:
- `arraybuffer-transfer` feature (actual detaching)
- `resizable-arraybuffer` feature (actual resizing)
- `SharedArrayBuffer` handling

**Estimated passes:** 15-25 of 32 direct tests (tests not requiring actual transfer/resize features)

---

## Verification

```bash
# Build
cargo build --release

# Run specific tests
uv run python scripts/run-test262.py test262/test/built-ins/ArrayBuffer/prototype/detached/
uv run python scripts/run-test262.py test262/test/built-ins/ArrayBuffer/prototype/resizable/
uv run python scripts/run-test262.py test262/test/built-ins/ArrayBuffer/prototype/maxByteLength/

# Run full ArrayBuffer suite
uv run python scripts/run-test262.py test262/test/built-ins/ArrayBuffer/

# Check for cascading improvements in DataView/TypedArray
uv run python scripts/run-test262.py test262/test/built-ins/DataView/
uv run python scripts/run-test262.py test262/test/built-ins/TypedArray/
```

---

## Files Summary

| File | Changes |
|------|---------|
| `src/interpreter/builtins/typedarray.rs` | Add 3 getters to ArrayBuffer.prototype |
| `README.md` | Update test count |
| `PLAN.md` | Add implementation entry |
