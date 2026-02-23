# Array Exotic [[DefineOwnProperty]] Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement spec-compliant Array exotic `[[DefineOwnProperty]]` (§10.4.2.1) and `ArraySetLength` (§10.4.2.4) so that `Object.defineProperties` (and any other caller of `define_own_property`) handles Array `.length` and index properties correctly.

**Architecture:** Create two new methods on `Interpreter` — `array_set_length()` and `array_define_own_property()` — that centralize Array exotic logic currently partially inlined in `Object.defineProperty`. Update both `Object.defineProperty` and `Object.defineProperties` to use these methods. The core `define_own_property()` on `JsObjectData` stays unchanged as `OrdinaryDefineOwnProperty`.

**Tech Stack:** Rust, ECMA-262 spec sections §10.4.2.1 and §10.4.2.4

---

### Task 1: Add `array_set_length()` method to Interpreter

**Files:**
- Modify: `src/interpreter/mod.rs` — add method to `impl Interpreter`

**Step 1: Implement `array_set_length()`**

Add this method to `impl Interpreter` (near other property-related helpers). It implements §10.4.2.4 `ArraySetLength(A, Desc)`:

```rust
/// §10.4.2.4 ArraySetLength(A, Desc)
/// Returns Ok(true) on success, Ok(false) on failure, Err on RangeError/TypeError.
pub(crate) fn array_set_length(
    &mut self,
    obj_id: usize,
    desc: PropertyDescriptor,
) -> Result<bool, JsValue> {
    let obj = self.get_object_by_id(obj_id).unwrap();

    // Step 1: If Desc does not have [[Value]], return OrdinaryDefineOwnProperty(A, "length", Desc)
    if desc.value.is_none() {
        let result = obj.borrow_mut().define_own_property("length".to_string(), desc);
        return Ok(result);
    }

    // Step 2: Let newLenDesc be a copy of Desc
    // Step 3: Let newLen = ToUint32(Desc.[[Value]])
    let new_len_val = desc.value.as_ref().unwrap().clone();
    let number_len = self.to_number_value(&new_len_val)?;  // Step 4
    let new_len = number_len as u32;

    // Step 5: If SameValueZero(newLen, numberLen) is false, throw RangeError
    if (new_len as f64) != number_len || number_len < 0.0 || number_len.is_nan() || number_len.is_infinite() {
        return Err(self.create_error("RangeError", "Invalid array length"));
    }

    // Build newLenDesc with [[Value]] = newLen
    let new_len_desc = PropertyDescriptor {
        value: Some(JsValue::Number(new_len as f64)),
        ..desc
    };

    // Step 6: Let oldLenDesc = OrdinaryGetOwnProperty(A, "length")
    let (old_len, old_len_writable) = {
        let b = obj.borrow();
        let old_len = b.properties.get("length")
            .and_then(|d| d.value.as_ref())
            .and_then(|v| if let JsValue::Number(n) = v { Some(*n as u32) } else { None })
            .unwrap_or(0);
        let writable = b.properties.get("length")
            .map(|d| d.writable != Some(false))
            .unwrap_or(true);
        (old_len, writable)
    };

    // Step 7: If newLen >= oldLen, return OrdinaryDefineOwnProperty(A, "length", newLenDesc)
    if new_len >= old_len {
        let result = obj.borrow_mut().define_own_property("length".to_string(), new_len_desc);
        return Ok(result);
    }

    // Step 8: If oldLenDesc.[[Writable]] is false, return false
    if !old_len_writable {
        return Ok(false);
    }

    // Step 9-10: Handle the writable dance
    let new_writable = if new_len_desc.writable == Some(false) {
        // Step 10: Temporarily keep writable true during deletion
        false
    } else {
        true
    };

    // Create desc with writable=true for the define step
    let temp_desc = PropertyDescriptor {
        writable: Some(true),
        ..new_len_desc
    };

    // Step 12: succeeded = OrdinaryDefineOwnProperty(A, "length", tempDesc)
    if !obj.borrow_mut().define_own_property("length".to_string(), temp_desc) {
        return Ok(false);
    }

    // Step 13-14: Delete elements from oldLen-1 downward
    let mut succeeded_all = true;
    {
        let mut b = obj.borrow_mut();
        // Collect index keys >= newLen, sorted descending
        let mut idx_keys: Vec<u32> = b.properties.keys()
            .filter_map(|k| k.parse::<u32>().ok().filter(|&idx| idx >= new_len))
            .collect();
        idx_keys.sort_unstable_by(|a, b_val| b_val.cmp(a));

        for idx in idx_keys {
            let k = idx.to_string();
            let is_non_configurable = b.properties.get(&k)
                .map(|d| d.configurable == Some(false))
                .unwrap_or(false);
            if is_non_configurable {
                // Step 14.a: Set length to idx + 1
                b.properties.insert(
                    "length".to_string(),
                    PropertyDescriptor {
                        value: Some(JsValue::Number((idx + 1) as f64)),
                        writable: Some(if new_writable { true } else { false }),
                        enumerable: Some(false),
                        configurable: Some(false),
                        get: None,
                        set: None,
                    },
                );
                if let Some(ref mut elems) = b.array_elements {
                    elems.truncate((idx + 1) as usize);
                }
                succeeded_all = false;
                break;
            }
            b.properties.remove(&k);
            b.property_order.retain(|pk| pk != &k);
        }

        if succeeded_all {
            if let Some(ref mut elems) = b.array_elements {
                elems.truncate(new_len as usize);
            }
        }
    }

    // Step 15: If !newWritable, set [[Writable]] to false
    if !new_writable {
        let mut b = obj.borrow_mut();
        if let Some(len_desc) = b.properties.get_mut("length") {
            len_desc.writable = Some(false);
        }
    }

    // Step 16: If not all deletions succeeded, return false
    if !succeeded_all {
        return Ok(false);
    }

    Ok(true)
}
```

**Step 2: Build and verify compilation**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully (method not yet called)

**Step 3: Commit**

```bash
git add src/interpreter/mod.rs
git commit -m "Add array_set_length() implementing §10.4.2.4 ArraySetLength"
```

---

### Task 2: Add `array_define_own_property()` method to Interpreter

**Files:**
- Modify: `src/interpreter/mod.rs` — add method to `impl Interpreter`

**Step 1: Implement `array_define_own_property()`**

Add this method to `impl Interpreter` right after `array_set_length()`. It implements §10.4.2.1:

```rust
/// §10.4.2.1 ArrayDefineOwnProperty(A, P, Desc)
/// Caller must verify obj is an Array before calling.
/// Returns Ok(true) on success, Ok(false) on failure, Err on throws.
pub(crate) fn array_define_own_property(
    &mut self,
    obj_id: usize,
    key: String,
    desc: PropertyDescriptor,
) -> Result<bool, JsValue> {
    // Step 1: If P is "length", return ArraySetLength(A, Desc)
    if key == "length" {
        return self.array_set_length(obj_id, desc);
    }

    // Step 2: If P is an array index
    if let Ok(index) = key.parse::<u32>() {
        // Valid array index: 0 to 2^32-2
        if index <= 0xFFFFFFFE {
            let obj = self.get_object_by_id(obj_id).unwrap();
            let (old_len, len_not_writable) = {
                let b = obj.borrow();
                let old_len = b.properties.get("length")
                    .and_then(|d| d.value.as_ref())
                    .and_then(|v| if let JsValue::Number(n) = v { Some(*n as u32) } else { None })
                    .unwrap_or(0);
                let len_not_writable = b.properties.get("length")
                    .map(|d| d.writable == Some(false))
                    .unwrap_or(false);
                (old_len, len_not_writable)
            };

            // Step 2.c-d: If index >= oldLen and length is non-writable, return false
            if index >= old_len && len_not_writable {
                return Ok(false);
            }

            // Step 2.e: succeeded = OrdinaryDefineOwnProperty(A, P, Desc)
            let succeeded = obj.borrow_mut().define_own_property(key, desc);
            if !succeeded {
                return Ok(false);
            }

            // Step 2.g: If index >= oldLen, set length to index + 1
            if index >= old_len {
                let new_len = index + 1;
                let mut b = obj.borrow_mut();
                if let Some(len_desc) = b.properties.get_mut("length") {
                    len_desc.value = Some(JsValue::Number(new_len as f64));
                }
                // Also update array_elements length
                if let Some(ref mut elems) = b.array_elements {
                    while elems.len() < new_len as usize {
                        elems.push(JsValue::Undefined);
                    }
                }
            }

            return Ok(true);
        }
    }

    // Step 3: Return OrdinaryDefineOwnProperty(A, P, Desc)
    let obj = self.get_object_by_id(obj_id).unwrap();
    Ok(obj.borrow_mut().define_own_property(key, desc))
}
```

**Step 2: Build and verify compilation**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/interpreter/mod.rs
git commit -m "Add array_define_own_property() implementing §10.4.2.1"
```

---

### Task 3: Refactor Object.defineProperty to use `array_define_own_property()`

**Files:**
- Modify: `src/interpreter/builtins/mod.rs:3902-4038` — replace inline Array logic

**Step 1: Replace inline Array logic in Object.defineProperty**

In `src/interpreter/builtins/mod.rs`, find the `Object.defineProperty` closure (starts around line 3846). Replace the section from line 3900 (`match interp.to_property_descriptor(...)`) through line 4038 with:

```rust
match interp.to_property_descriptor(&desc_val) {
    Ok(desc) => {
        let is_array = obj.borrow().class_name == "Array";
        if is_array {
            match interp.array_define_own_property(o.id, key, desc) {
                Ok(true) => {}
                Ok(false) => {
                    return Completion::Throw(interp.create_type_error(
                        "Cannot define property, object is not extensible or property is non-configurable",
                    ));
                }
                Err(e) => return Completion::Throw(e),
            }
        } else if !obj.borrow_mut().define_own_property(key, desc) {
            return Completion::Throw(interp.create_type_error(
                "Cannot define property, object is not extensible or property is non-configurable",
            ));
        }
    }
    Err(Some(e)) => return Completion::Throw(e),
    Err(None) => {}
}
```

**Step 2: Build and verify**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully

**Step 3: Run the 3 failing defineProperty tests**

Run: `uv run python scripts/run-test262.py -j 128 test262/test/built-ins/Object/defineProperty/15.2.3.6-4-124.js test262/test/built-ins/Object/defineProperty/15.2.3.6-4-167.js test262/test/built-ins/Object/defineProperty/15.2.3.6-4-181.js`
Expected: All 3 tests (6 scenarios) should now pass. If any regress, debug before proceeding.

**Step 4: Run full Object.defineProperty suite**

Run: `uv run python scripts/run-test262.py -j 128 test262/test/built-ins/Object/defineProperty/`
Expected: No regressions from current 1128/1131 passing (should be 1131/1131)

**Step 5: Commit**

```bash
git add src/interpreter/builtins/mod.rs
git commit -m "Refactor Object.defineProperty to use array_define_own_property()"
```

---

### Task 4: Update Object.defineProperties to use `array_define_own_property()`

**Files:**
- Modify: `src/interpreter/builtins/mod.rs:4944-4951` — update the descriptor application loop

**Step 1: Update the apply-descriptors loop**

In the `Object.defineProperties` closure, replace the loop at line 4945-4951:

```rust
// Apply all descriptors
for (key, desc) in descriptors {
    let is_array = if let Some(target_obj) = interp.get_object(t.id) {
        target_obj.borrow().class_name == "Array"
    } else {
        false
    };

    if is_array {
        match interp.array_define_own_property(t.id, key, desc) {
            Ok(true) => {}
            Ok(false) => {
                return Completion::Throw(interp.create_type_error(
                    "Cannot define property, object is not extensible or property is non-configurable",
                ));
            }
            Err(e) => return Completion::Throw(e),
        }
    } else if let Some(target_obj) = interp.get_object(t.id)
        && !target_obj.borrow_mut().define_own_property(key, desc) {
            return Completion::Throw(interp.create_type_error(
                "Cannot define property, object is not extensible or property is non-configurable",
            ));
        }
}
```

**Step 2: Build and verify**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully

**Step 3: Run the 55 failing defineProperties tests**

Run: `uv run python scripts/run-test262.py -j 128 test262/test/built-ins/Object/defineProperties/`
Expected: Most/all of the 55 previously failing tests should now pass. Target: 632/632 (from 577/632).

**Step 4: Commit**

```bash
git add src/interpreter/builtins/mod.rs
git commit -m "Update Object.defineProperties to use array_define_own_property()"
```

---

### Task 5: Run full regression test and verify

**Files:**
- Modify: `README.md` — update pass count
- Modify: `PLAN.md` — update Object pass count
- Modify: `test262-pass.txt` — updated by test runner

**Step 1: Run full test262 suite**

Run: `cargo build --release && uv run python scripts/run-test262.py -j 128`
Expected: ~87,537+ passes (at least 57 new passes), zero regressions from 87,480 baseline.

**Step 2: If any regressions, investigate and fix**

Compare new test262-pass.txt against old to find any tests that stopped passing. Common issues:
- The `is_array` check needs to handle Proxy-of-Array (probably not — Proxy has its own defineProperty trap path)
- `array_elements` sync: if `define_own_property` modifies a numeric property, `array_elements` may be out of sync — check if the existing `define_own_property` already handles this

**Step 3: Update README.md pass count**

Update the test262 progress table in README.md with the new pass count and percentage.

**Step 4: Update PLAN.md**

Update the Object pass count row in the "Current Built-in Status" table.

**Step 5: Commit**

```bash
git add README.md PLAN.md test262-pass.txt
git commit -m "Update test262 results: Array exotic [[DefineOwnProperty]] (+N passes)"
```

---

### Task 6: Run lint and clean up

**Step 1: Run linter**

Run: `./scripts/lint.sh`
Expected: No new lint warnings

**Step 2: Fix any warnings**

If there are unused variable warnings or clippy issues from the refactor, fix them.

**Step 3: Final commit if needed**

```bash
git add -A
git commit -m "Fix lint warnings from array exotic defineownproperty"
```
