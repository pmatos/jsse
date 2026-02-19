# Async Generator yield* Getter-Awareness Fix

## Problem

179 test262 test files (~358 scenarios) fail because the iterator protocol methods use raw property access (`get_property()`) instead of getter-aware access (`get_object_property()`). This means accessor properties (getters) on iterator objects are never invoked.

The failing tests use getter-based iterators to observe the exact operation ordering required by the spec (e.g., `get next`, `call next`, `get done`, `get value`).

## Root Cause

Seven iterator protocol methods in `src/interpreter/builtins/mod.rs` and one code path in `src/interpreter/eval.rs` use `get_property()` (raw data access) where the spec requires `[[Get]]` (getter-aware access):

| Method | Line | Current | Should Be |
|--------|------|---------|-----------|
| `get_iterator()` | 5041 | `get_property(key)` | `get_object_property()` |
| `get_async_iterator()` | 5092 | `get_property(key)` | `get_object_property()` |
| `iterator_next()` | 5229 | `get_property_descriptor("next")` | `get_object_property()` |
| `iterator_next_with_value()` | 5259 | `get_property_descriptor("next")` | `get_object_property()` |
| `iterator_return()` | 5324 | `get_property("return")` | `get_object_property()` |
| `iterator_throw()` | 5358 | `get_property("throw")` | `get_object_property()` |
| `iterator_close()` | 5388 | `get_property("return")` | `get_object_property()` |
| yield* eval.rs | 518-519 | `get_property("done"/"value")` | `iterator_complete()`/`iterator_value()` |

`iterator_complete()` and `iterator_value()` are already correct (use `get_object_property`).

## Fix

Replace raw property access with getter-aware `get_object_property()` in each method. The pattern is:

```rust
// Before (wrong):
let val = obj_data.borrow().get_property(key);

// After (correct):
let val = match self.get_object_property(obj_id, key, receiver) {
    Completion::Normal(v) => v,
    Completion::Throw(e) => return Err(e),
    _ => JsValue::Undefined,
};
```

For the yield* path in eval.rs, replace inline `get_property("done")`/`get_property("value")` with calls to the already-correct `self.iterator_complete()` and `self.iterator_value()`.

## Expected Impact

- ~358 new passes from yield-star tests across 6+ directory variants
- Additional passes from for-of, spread, destructuring using getter-based iterators
- Estimated total: 350-450 new passes

## Risk

Low. `get_object_property()` is the standard pattern used throughout the codebase. The main concern is borrow conflicts — `get_object_property()` takes `&mut self` so we must not hold a borrow on the object store when calling it. The existing `iterator_complete()`/`iterator_value()` already demonstrate the correct borrow pattern.
