# Array Exotic [[DefineOwnProperty]] — Design

## Problem

Array exotic `[[DefineOwnProperty]]` (§10.4.2.1) and `ArraySetLength` (§10.4.2.4) are partially inlined in `Object.defineProperty` but absent from the core `define_own_property()` method on `JsObjectData`. This means `Object.defineProperties` (and any other path calling `define_own_property()` directly) bypasses all Array-specific logic.

54 of 55 failing `Object.defineProperties` tests + 3 `Object.defineProperty` edge cases are caused by this single gap.

## Spec References

- §10.4.2.1 `ArrayDefineOwnProperty(A, P, Desc)` — dispatches on key type
- §10.4.2.4 `ArraySetLength(A, Desc)` — handles "length" property definition with ToUint32 validation, element deletion, writable dance

## Solution

### New Methods on Interpreter

**`array_define_own_property(obj_id, key, desc) -> Result<bool, JsValue>`**

Implements §10.4.2.1:
1. If key == "length": call `array_set_length()`
2. If key is array index (parseable as u32, < 2^32): check old length writable, delegate to OrdinaryDefineOwnProperty, auto-extend length if index >= oldLen
3. Else: delegate to OrdinaryDefineOwnProperty

**`array_set_length(obj_id, desc) -> Result<bool, JsValue>`**

Implements §10.4.2.4:
1. If desc has no [[Value]]: delegate to OrdinaryDefineOwnProperty
2. ToNumber + ToUint32 validation — RangeError if mismatch
3. If newLen >= oldLen: just define length via OrdinaryDefineOwnProperty
4. If oldLen writable is false: return false
5. Writable dance: temporarily keep writable=true during deletion, set false after
6. Delete elements oldLen-1 downward, stop at non-configurable
7. If blocked by non-configurable: set length to blocker+1, optionally writable=false, return false

### Changes to Existing Code

- `Object.defineProperty` (builtins/mod.rs:3902-4038): replace inline Array logic with call to `array_define_own_property()`
- `Object.defineProperties` (builtins/mod.rs:4945-4951): check if target is Array, call `array_define_own_property()` instead of raw `define_own_property()`
- `define_own_property()` (types.rs): unchanged — stays as OrdinaryDefineOwnProperty

### Impact

- 57+ direct test passes
- Zero expected regressions (centralizing existing logic)
