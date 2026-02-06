# Property Descriptor Fixes Plan

## Summary of Findings

75 failures in `Object/defineProperty/` (52) and `Object/getOwnPropertyDescriptor/` (23), plus ~263 additional Object-area failures in `defineProperties` (59), `getOwnPropertyDescriptors` (13), and other areas. Many of these stem from a few cross-cutting root causes.

---

## Bug 1: Built-in methods added with `insert_value` instead of `insert_builtin`

**Root cause:** All Object.* static methods are registered using `obj_func.borrow_mut().insert_value(...)`, which uses `PropertyDescriptor::data_default()` — writable=true, **enumerable=true**, configurable=true. Per spec, built-in function properties should be writable=true, **enumerable=false**, configurable=true. The correct helper `insert_builtin()` already exists but isn't used.

**Location:** `src/interpreter/builtins/mod.rs` — lines 2995, 3135, 3209, 3234, 3343, 3431, 3476, 3520, 3601, 3667, 3683, 3741, 3771, 3828, 3882, 3908, 3934, 3956, 3977, 4156, 4254

**Tests blocked:**
- defineProperty: 15.2.3.6-4-{570,598,599,600,601,602,603,604,605,606,607,608,609,610} (13 tests)
- getOwnPropertyDescriptor: 15.2.3.3-4-{14,15,16,17,18,19,20,21,22,23,24,25,26,61} (14 tests)
- Likely also in getOwnPropertyDescriptors/function-property-descriptor.js
- Cascading to other built-in objects using `insert_value` elsewhere (String, Array, etc.)

**Fix:** Change all `insert_value` calls for built-in static functions to `insert_builtin`. Must audit ALL builtins files, not just Object.

**Estimated gain:** ~27+ tests directly, more cascading

**Risk:** Very low — `insert_builtin` is already well-tested

---

## Bug 2: Global value properties (NaN, Infinity, undefined) have wrong descriptors

**Root cause:** Global `NaN`, `Infinity`, `undefined` are defined as writable=true, enumerable=true, configurable=true. Per §19.1: NaN/Infinity/undefined should be writable=false, enumerable=false, configurable=false.

**Location:** Wherever the global object setup defines NaN/Infinity/undefined in `src/interpreter/builtins/mod.rs` or `src/interpreter/mod.rs`.

**Tests blocked:**
- getOwnPropertyDescriptor: 15.2.3.3-4-{178,179,180} (3 tests)

**Fix:** Use `insert_property` with `PropertyDescriptor::data(val, false, false, false)` for these globals.

**Estimated gain:** 3 tests

**Risk:** Very low

---

## Bug 3: `from_property_descriptor` omits `set`/`get` when they're `None` on accessor descriptors

**Root cause:** In `from_property_descriptor()` (mod.rs:271), the code uses `if let Some(ref s) = desc.set` which means if an accessor was defined without `set`, the returned descriptor object won't have a `set` property at all. Per spec §6.2.5.4 FromPropertyDescriptor, accessor descriptors must always include both `get` and `set` (even if undefined).

Similarly, data descriptors should always include `value` and `writable` even if they happen to be None in our internal representation (though this is less common).

**Location:** `src/interpreter/mod.rs:271-296`

**Tests blocked:**
- defineProperty: 15.2.3.6-4-206 (desc.hasOwnProperty("set") fails)
- getOwnPropertyDescriptor: 15.2.3.3-4-{249,250} (missing set/get)

**Fix:** In `from_property_descriptor`, determine if the descriptor is an accessor or data descriptor, and always include all four relevant properties. For accessor: always include `get`, `set`, `enumerable`, `configurable`. For data: always include `value`, `writable`, `enumerable`, `configurable`.

**Estimated gain:** 3+ tests

**Risk:** Low

---

## Bug 4: `to_primitive` returns `"[object Object]"` instead of throwing TypeError

**Root cause:** In `to_primitive()` (eval.rs:1015), when both `valueOf` and `toString` return objects (not primitive), the spec says we must throw TypeError. Instead, the code falls through to `JsValue::String("[object Object]")`.

**Location:** `src/interpreter/eval.rs:1009-1015`

**Tests blocked:**
- defineProperty: 15.2.3.6-2-47 (ToPrimitive fails to throw)
- getOwnPropertyDescriptor: 15.2.3.3-2-46

**Fix:** Remove the fallback at line 1015 and return a TypeError instead.

**Estimated gain:** 2+ tests (plus cascade to many other ToPrimitive scenarios)

**Risk:** Medium — could affect other code paths that rely on the current fallback behavior. Need to check for regressions. The `primitive_value` fallback at line 1010-1014 should remain but the final catch-all should throw.

---

## Bug 5: `in` operator doesn't use `to_property_key_string` for symbols

**Root cause:** In `eval_binary` for `BinaryOp::In` (eval.rs:1358), the code uses `to_js_string(left)` which for symbols gives the display string `"Symbol(test)"`, not the unique property key `"Symbol(sym_42)"`. Properties are stored using `to_property_key_string` which generates unique keys.

**Location:** `src/interpreter/eval.rs:1355-1367`

**Tests blocked:**
- defineProperty: symbol-data-property-{configurable,default-non-strict,default-strict,writable} (4 tests)

**Fix:** Change `to_js_string(left)` to `to_property_key_string(left)` in the `BinaryOp::In` handler.

**Estimated gain:** 4 tests

**Risk:** Very low

---

## Bug 6: Array `defineProperty` on indices doesn't update length

**Root cause:** In the Object.defineProperty handler (builtins/mod.rs:2930-2984), when defining a property on an array with a numeric key >= current length, the length should be updated to key+1. Currently only the "length" property itself is special-cased, not index properties.

Per §10.4.2.1 ArrayDefineOwnProperty: if the property name is an array index and the index >= oldLen, then set newLen = index + 1 and update the length property.

**Location:** `src/interpreter/builtins/mod.rs:2930-2984`

**Tests blocked:**
- defineProperty: 15.2.3.6-4-{183,275,276} and likely more
- defineProperties: many tests (15.2.3.7-6-a-127, etc.)

**Fix:** After calling `define_own_property` for a non-length key on an array, check if the key is a valid array index. If so, check if the index >= current length and update length to index + 1.

**Estimated gain:** ~5+ tests

**Risk:** Low-medium

---

## Bug 7: Array length reduction doesn't check non-configurable elements

**Root cause:** In ArraySetLength (builtins/mod.rs:2955-2964), when reducing length, it just removes all elements >= newLen without checking if any are non-configurable. Per §10.4.2.4 step 3.l: if delete of an element fails (non-configurable), set length to that element's index + 1 and throw TypeError.

**Location:** `src/interpreter/builtins/mod.rs:2932-2977`

**Tests blocked:**
- defineProperty: 15.2.3.6-4-{116,117,168,169,170,172,173,174,176,177,188,189} (~12 tests)
- defineProperties: 15.2.3.7-6-a-{112,113} and many more (~20+ tests)

**Fix:** Before removing elements in the length reduction loop, iterate from oldLen-1 downward and check if each element is configurable. If not, set final length to that index + 1, throw TypeError.

**Estimated gain:** ~30+ tests

**Risk:** Medium — complex logic, need to match spec precisely

---

## Bug 8: Array length value not converted via ToUint32 (objects with toString/valueOf)

**Root cause:** In ArraySetLength (builtins/mod.rs:2937), the code uses `to_number(&new_len_val)` which is the non-calling version — it doesn't invoke ToPrimitive for objects. Should use `interp.to_number_coerce(&new_len_val)` or `interp.to_number_value(&new_len_val)`.

**Location:** `src/interpreter/builtins/mod.rs:2937`

**Tests blocked:**
- defineProperty: 15.2.3.6-4-{146,147,148,149,150,151} (6 tests)

**Fix:** Replace `to_number(&new_len_val)` with `interp.to_number_coerce(&new_len_val)` (which calls ToPrimitive).

**Estimated gain:** 6 tests

**Risk:** Low

---

## Bug 9: String exotic objects don't expose index/length properties via `get_own_property`

**Root cause:** `get_own_property` (types.rs:799) only looks in `self.properties`. For String exotic objects, per §10.4.3.1, index properties ("0", "1", etc.) and "length" should be virtual own properties. `has_own_property` already handles this (types.rs:807-815), but `get_own_property` doesn't.

**Location:** `src/interpreter/types.rs:799-801`

**Tests blocked:**
- getOwnPropertyDescriptor: 15.2.3.3-{3-14,4-192}, primitive-string.js (3 tests)
- getOwnPropertyDescriptors: primitive-strings.js

**Fix:** In `get_own_property`, if key is "length" and class_name=="String", return a descriptor for the string length. If key is a valid index within the string, return a descriptor for that character. These are non-writable, non-configurable (length also non-enumerable; indices are enumerable).

**Estimated gain:** 4+ tests

**Risk:** Low — need to return a synthesized PropertyDescriptor, which means the return type may need to change from `Option<&PropertyDescriptor>` to `Option<PropertyDescriptor>` (owned vs borrowed). This ripples through callers.

---

## Bug 10: Array holes treated as `Undefined` instead of absent

**Root cause:** In `eval_array_literal` (eval.rs:6007), elided elements push `JsValue::Undefined` into the values vector. Then `create_array` stores them all as own properties. Per spec, `[0,,2]` should NOT have own property "1" — index 1 is a hole (no property at all).

**Location:** `src/interpreter/eval.rs:6007` and `src/interpreter/builtins/array.rs:2338`

**Tests blocked:**
- defineProperty: 15.2.3.6-4-{159,160} (at least)

**Fix:** Use `Option<JsValue>` in the values vector, with `None` for holes. Skip storing properties for `None` entries in `create_array`. This is a broader change that affects array_elements storage.

**Estimated gain:** 2+ tests

**Risk:** Medium-high — array_elements is used extensively. Many operations assume contiguous indexing.

---

## Implementation Order (safest/smallest first)

| Order | Bug | Est. Gain | Risk | Effort |
|-------|-----|-----------|------|--------|
| 1 | Bug 5: `in` operator symbol keys | 4 | Very low | Tiny |
| 2 | Bug 1: `insert_value` → `insert_builtin` for builtins | 27+ | Very low | Small (many locations) |
| 3 | Bug 2: Global NaN/Infinity/undefined descriptors | 3 | Very low | Tiny |
| 4 | Bug 4: `to_primitive` TypeError fallback | 2+ | Medium | Small |
| 5 | Bug 8: ArraySetLength ToUint32 conversion | 6 | Low | Tiny |
| 6 | Bug 3: `from_property_descriptor` completeness | 3+ | Low | Small |
| 7 | Bug 6: Array index defineProperty length update | 5+ | Low-medium | Small |
| 8 | Bug 7: Array length non-configurable check | 30+ | Medium | Medium |
| 9 | Bug 9: String exotic `get_own_property` | 4+ | Low | Medium (API change) |
| 10 | Bug 10: Array holes | 2+ | Medium-high | Large |

**Total estimated direct gains: ~86+ tests**
**With cascading effects (defineProperties shares bugs with defineProperty, other builtins share Bug 1): potentially 150+ tests**
