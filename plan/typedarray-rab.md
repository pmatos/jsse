# TypedArray Resizable ArrayBuffer Method Compliance Plan

**Estimated impact: ~232 new test262 passes**
**Status: Phase 1 merged** — Initial OOB checks added (+162 passes, TypedArray: 2,572→2,732/2,860, 95.5%). Remaining work: post-coercion re-validation, iteration length caching, subarray/set edge cases.

## Problem Summary

TypedArray prototype methods currently only check `ta.is_detached.get()` when validating. The spec's `ValidateTypedArray` (§23.2.4.1) also requires checking `IsTypedArrayOutOfBounds(taRecord)` and throwing TypeError if true. When a resizable ArrayBuffer is shrunk so a fixed-length TypedArray's byte range exceeds the buffer size, ALL methods must throw TypeError.

The helpers `is_typed_array_out_of_bounds` and `typed_array_length` in `types.rs` already implement correct logic — they just need to be called at the right validation points.

## Three Categories of Bugs

**Category A: Missing initial OOB check (ValidateTypedArray)**
- Every method checks `if ta.is_detached.get()` but NOT `if is_typed_array_out_of_bounds(&ta)`
- Most common bug, affects ALL methods

**Category B: Missing post-coercion re-validation**
- Methods like fill, copyWithin, slice, with must re-check OOB and recompute length AFTER argument coercions that could trigger a resize callback

**Category C: Iteration length caching**
- Callback methods (forEach, map, find, etc.) must cache `len` at start, not re-evaluate `typed_array_length` each iteration

## Files to Modify

- **`src/interpreter/builtins/typedarray.rs`** — All TypedArray prototype method implementations (~5700 lines)
- **`src/interpreter/types.rs`** — May add `validate_typed_array` convenience wrapper

## Task 0: Create `validate_typed_array` Helper

In `types.rs`, create a function combining detach + OOB checks:

```rust
pub(crate) fn validate_typed_array(ta: &TypedArrayInfo) -> Result<(), &'static str> {
    if ta.is_detached.get() {
        return Err("typed array is detached");
    }
    if is_typed_array_out_of_bounds(ta) {
        return Err("typed array is out of bounds");
    }
    Ok(())
}
```

## Task 1: Fix `extract_ta_and_callback` Helper (~line 4875)

Currently at line 4886 only checks `ta.is_detached.get()`. Add OOB check. This single change fixes initial validation for 12 callback methods: find, findIndex, findLast, findLastIndex, forEach, map, filter, every, some, reduce, reduceRight.

**Pattern:**
```rust
// Before:
if ta.is_detached.get() { ... }
// After:
if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) { ... }
```

## Task 2: Fix Simple Non-Callback Methods

Each inlines validation. Change `if ta.is_detached.get()` to `if ta.is_detached.get() || is_typed_array_out_of_bounds(ta)`:

- `at` (~line 1153)
- `indexOf` (~line 1673)
- `lastIndexOf` (~line 1726)
- `includes` (~line 1781)
- `reverse` (~line 1838)
- `sort` (~line 1878)
- `join` (~line 1995)
- `toString` (~line 2043)
- `toLocaleString` (~line 2087)
- `toReversed` (~line 2166)
- `toSorted` (~line 2226)
- `with` (~line 2504)
- `slice` (~line 1417)
- `copyWithin` (~line 1509)
- `fill` (~line 1588)

## Task 3: Fix `set` Method (~line 1181)

After offset coercion (line 1213), add OOB check for target TypedArray. Also add OOB check for source TypedArray at line 1228 when source is a TypedArray.

## Task 4: Fix `subarray` Method (~line 1331)

Special case per spec §23.2.3.28: does NOT throw TypeError on OOB arrays. Instead uses `srcLength = 0`. The existing `typed_array_length` already returns 0 for OOB, but verify edge cases where `byte_offset > buf_len` for the new subarray.

## Task 5: Fix Iterator Creation Methods (lines ~2627-2710)

`values`, `keys`, `entries` at line 2634 check `if ta.is_detached.get()`. Add OOB check. The iterator's `next()` method in `iterators.rs` already handles OOB correctly (returns done:true).

## Task 6: Fix Post-Coercion Re-validation

Methods that can trigger buffer resize during argument coercion:

- **`fill`** (partially done at lines 1637-1649): verify re-validation matches spec, check OOB, recompute length
- **`copyWithin`** (lines 1552-1557): re-checks detach but NOT OOB. Add OOB check and recompute length per spec steps 17-19
- **`slice`** (after TypedArraySpeciesCreate): spec step 16 re-validates. Add re-check after species create
- **`set`** (array-like path): `typed_array_set_index` handles bounds; initial OOB from Task 3 is sufficient

## Task 7: Fix `with` Method Re-validation

Per spec step 9, `with` uses `IsValidIntegerIndex` after coercions. Current code at line 2543 uses original `len` computed before coercions. After value coercion (which may resize), re-check index against live TypedArrayInfo using `is_valid_integer_index`. Keep original `len` for result creation.

## Task 8: Fix Callback Methods Iteration Length Caching

Methods using `for i in 0..typed_array_length(&ta)` re-evaluate length each iteration. Per spec, loop bound is fixed at validation step. Fix to `let len = typed_array_length(&ta); for i in 0..len`:

Affected methods:
- forEach, map, filter, every, some
- find, findIndex, findLast, findLastIndex
- reduce, reduceRight

When buffer shrinks during callback, elements beyond new size read as `undefined` (0 for numeric, 0n for BigInt). `typed_array_get_index` already handles this.

## Task 9: Test Incrementally

After each task, run:
```bash
uv run python scripts/run-test262.py test262/test/built-ins/TypedArray/prototype/<method>/ -j 128
```

## Implementation Order

1. Task 0: Create validate_typed_array helper
2. Task 1: Fix extract_ta_and_callback (fixes 12 methods at once)
3. Task 8: Cache iteration length in callback methods
4. Task 2: Fix 15 simple non-callback methods
5. Task 5: Fix iterator creation methods
6. Task 3: Fix set method
7. Task 4: Fix subarray method
8. Task 6: Fix post-coercion re-validation (fill, copyWithin, slice)
9. Task 7: Fix with method re-validation
10. Task 9: Full TypedArray test run

## Critical Files

- `src/interpreter/builtins/typedarray.rs` — ALL method implementations
- `src/interpreter/types.rs` — `TypedArrayInfo`, `typed_array_length`, `is_typed_array_out_of_bounds`, `is_valid_integer_index` helpers (already correct)
- `src/interpreter/builtins/iterators.rs` — TypedArray iterator next() (already handles OOB, verify)
- `test262/harness/resizableArrayBufferUtils.js` — Test harness utilities
