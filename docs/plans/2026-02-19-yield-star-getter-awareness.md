# Iterator Protocol Getter-Awareness Fix — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 7 iterator protocol methods to use getter-aware property access, unlocking ~350+ new test262 passes.

**Architecture:** Replace raw `get_property()` / `get_property_descriptor()` calls with `get_object_property()` (which invokes accessor getters and Proxy traps) in the iterator protocol methods in `builtins/mod.rs`, and replace inline done/value access in the yield* code path in `eval.rs` with the already-correct `iterator_complete()`/`iterator_value()`.

**Tech Stack:** Rust, tree-walking interpreter, test262 suite

---

### Task 1: Build baseline and record current pass count

**Files:** None modified

**Step 1: Build in release mode**

Run: `cargo build --release`
Expected: Successful build

**Step 2: Run yield-star tests to record baseline**

Run: `uv run python scripts/run-test262.py test262/test/language/statements/class/elements/async-gen-private-method-static/ -j $(nproc)`
Expected: Note the pass/fail counts for the yield-star subdirectory.

**Step 3: Run a broader baseline on async generators**

Run: `uv run python scripts/run-test262.py test262/test/language/expressions/async-generator/ test262/test/language/statements/async-generator/ -j $(nproc)`
Expected: Note pass/fail for async generator tests.

---

### Task 2: Fix `get_iterator()` — getter-aware Symbol.iterator lookup

**Files:**
- Modify: `src/interpreter/builtins/mod.rs` — `get_iterator()` method (~line 5035-5084)

**Step 1: Fix the Object branch**

In `get_iterator()`, replace the raw `get_property(key)` for the Object case with `get_object_property()`:

```rust
// BEFORE (line 5040-5048):
if let Some(obj_data) = self.get_object(o.id) {
    let val = obj_data.borrow().get_property(key);
    if matches!(val, JsValue::Undefined) {
        return Err(self.create_type_error("is not iterable"));
    }
    val
} else {
    return Err(self.create_type_error("is not iterable"));
}

// AFTER:
{
    let val = match self.get_object_property(o.id, key, obj) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => JsValue::Undefined,
    };
    if matches!(val, JsValue::Undefined) {
        return Err(self.create_type_error("is not iterable"));
    }
    val
}
```

Note: The `if let Some(key)` guard stays. Only the inner body changes. The `obj` parameter serves as `this_val` receiver.

**Step 2: Fix the String branch**

The String branch (line 5053-5068) uses `proto.borrow().get_property(key)`. This path looks up `Symbol.iterator` on `String.prototype`. Since `String.prototype[Symbol.iterator]` is a data property (not a getter), this is less critical but should still be correct. Replace with:

```rust
// BEFORE (line 5057):
let val = proto.borrow().get_property(key);

// AFTER:
let proto_id = proto.borrow().id.unwrap();
let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
let val = match self.get_object_property(proto_id, key, &proto_val) {
    Completion::Normal(v) => v,
    Completion::Throw(e) => return Err(e),
    _ => JsValue::Undefined,
};
```

**Step 3: Verify it compiles**

Run: `cargo build --release`
Expected: Successful build

**Step 4: Run quick sanity test**

Run: `./target/release/jsse -e 'for (let x of [1,2,3]) console.log(x)'`
Expected: Prints 1, 2, 3

**Step 5: Commit**

```
git add src/interpreter/builtins/mod.rs
git commit -m "Fix get_iterator() to use getter-aware property access"
```

---

### Task 3: Fix `get_async_iterator()` — getter-aware Symbol.asyncIterator lookup

**Files:**
- Modify: `src/interpreter/builtins/mod.rs` — `get_async_iterator()` method (~line 5086-5123)

**Step 1: Replace raw `get_property` with `get_object_property`**

```rust
// BEFORE (line 5090-5101):
JsValue::Object(o) => {
    if let Some(obj_data) = self.get_object(o.id) {
        let val = obj_data.borrow().get_property(key);
        if !matches!(val, JsValue::Undefined) {
            Some(val)
        } else {
            None
        }
    } else {
        None
    }
}

// AFTER:
JsValue::Object(o) => {
    let val = match self.get_object_property(o.id, key, obj) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => JsValue::Undefined,
    };
    if !matches!(val, JsValue::Undefined) {
        Some(val)
    } else {
        None
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build --release`
Expected: Successful build

**Step 3: Run getter-based async iterator test**

Run:
```
./target/release/jsse -e '
var obj = {
  get [Symbol.asyncIterator]() {
    console.log("getter called");
    return function() {
      return { next() { return Promise.resolve({done:true, value:42}); } };
    };
  }
};
async function* gen() { var v = yield* obj; console.log("v="+v); }
gen().next().then(r => console.log(JSON.stringify(r)));
'
```
Expected: `getter called` then `v=42` then `{"value":undefined,"done":true}` (or similar correct completion)

**Step 4: Commit**

```
git add src/interpreter/builtins/mod.rs
git commit -m "Fix get_async_iterator() to use getter-aware property access"
```

---

### Task 4: Fix `iterator_next()` and `iterator_next_with_value()` — getter-aware `next` lookup

**Files:**
- Modify: `src/interpreter/builtins/mod.rs` — both methods (~line 5226-5282)

**Step 1: Fix `iterator_next()`**

```rust
// BEFORE (line 5228-5232):
let next_fn = self.get_object(io.id).and_then(|obj| {
    obj.borrow()
        .get_property_descriptor("next")
        .and_then(|d| d.value)
});

// AFTER:
let next_fn = match self.get_object_property(io.id, "next", iterator) {
    Completion::Normal(v) if !matches!(v, JsValue::Undefined) => Some(v),
    Completion::Throw(e) => return Err(e),
    _ => None,
};
```

**Step 2: Fix `iterator_next_with_value()` — same pattern**

```rust
// BEFORE (line 5259-5262):
let next_fn = self.get_object(io.id).and_then(|obj| {
    obj.borrow()
        .get_property_descriptor("next")
        .and_then(|d| d.value)
});

// AFTER:
let next_fn = match self.get_object_property(io.id, "next", iterator) {
    Completion::Normal(v) if !matches!(v, JsValue::Undefined) => Some(v),
    Completion::Throw(e) => return Err(e),
    _ => None,
};
```

**Step 3: Verify it compiles**

Run: `cargo build --release`
Expected: Successful build

**Step 4: Run getter-based next() test**

Run:
```
./target/release/jsse -e '
var log = [];
var iter = {
  get next() { log.push("get next"); return function() { log.push("call next"); return {done:true, value:1}; }; }
};
var result = iter.next();
console.log(log.join(", "));
console.log(JSON.stringify(result));
'
```
Expected: Should show `get next, call next` and `{"done":true,"value":1}`

**Step 5: Commit**

```
git add src/interpreter/builtins/mod.rs
git commit -m "Fix iterator_next/iterator_next_with_value to use getter-aware next lookup"
```

---

### Task 5: Fix `iterator_return()`, `iterator_throw()`, `iterator_close()` — getter-aware method lookups

**Files:**
- Modify: `src/interpreter/builtins/mod.rs` — three methods (~line 5317-5396)

**Step 1: Fix `iterator_return()`**

```rust
// BEFORE (line 5323-5330):
let return_fn = self.get_object(io.id).and_then(|obj| {
    let val = obj.borrow().get_property("return");
    if matches!(val, JsValue::Object(_)) {
        Some(val)
    } else {
        None
    }
});

// AFTER:
let return_fn = match self.get_object_property(io.id, "return", iterator) {
    Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Some(v),
    Completion::Normal(_) => None,
    Completion::Throw(e) => return Err(e),
    _ => None,
};
```

**Step 2: Fix `iterator_throw()` — same pattern**

```rust
// BEFORE (line 5357-5364):
let throw_fn = self.get_object(io.id).and_then(|obj| {
    let val = obj.borrow().get_property("throw");
    if matches!(val, JsValue::Object(_)) {
        Some(val)
    } else {
        None
    }
});

// AFTER:
let throw_fn = match self.get_object_property(io.id, "throw", iterator) {
    Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Some(v),
    Completion::Normal(_) => None,
    Completion::Throw(e) => return Err(e),
    _ => None,
};
```

**Step 3: Fix `iterator_close()` — same pattern**

```rust
// BEFORE (line 5387-5394):
let return_fn = self.get_object(io.id).and_then(|obj| {
    let val = obj.borrow().get_property("return");
    if matches!(val, JsValue::Object(_)) {
        Some(val)
    } else {
        None
    }
});

// AFTER:
let return_fn = match self.get_object_property(io.id, "return", iterator) {
    Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Some(v),
    Completion::Normal(_) => None,
    Completion::Throw(e) => return Err(e),
    _ => None,
};
```

Note for `iterator_close`: it currently returns `JsValue`, not `Result`. The error from `get_object_property` should be propagated — this may require changing the return type to `Result<JsValue, JsValue>` or handling the error inline. Check callers to determine the right approach.

**Step 4: Verify it compiles**

Run: `cargo build --release`
Expected: Successful build

**Step 5: Commit**

```
git add src/interpreter/builtins/mod.rs
git commit -m "Fix iterator_return/throw/close to use getter-aware property access"
```

---

### Task 6: Fix yield* non-state-machine path in eval.rs — use iterator_complete/iterator_value

**Files:**
- Modify: `src/interpreter/eval.rs` — yield* delegation code (~line 516-526)

**Step 1: Replace inline property access with iterator_complete/iterator_value**

```rust
// BEFORE (line 516-526):
let (done_val, value) = if let JsValue::Object(ref ro) = next_result {
    if let Some(robj) = self.get_object(ro.id) {
        let d = robj.borrow().get_property("done");
        let v = robj.borrow().get_property("value");
        (d, v)
    } else {
        (JsValue::Undefined, JsValue::Undefined)
    }
} else {
    (JsValue::Undefined, JsValue::Undefined)
};
if to_boolean(&done_val) {
    break Completion::Normal(value);
}

// AFTER:
let done = match self.iterator_complete(&next_result) {
    Ok(d) => d,
    Err(e) => {
        self.gc_unroot_value(&iterator);
        return Completion::Throw(e);
    }
};
let value = match self.iterator_value(&next_result) {
    Ok(v) => v,
    Err(e) => {
        self.gc_unroot_value(&iterator);
        return Completion::Throw(e);
    }
};
if done {
    break Completion::Normal(value);
}
```

**Step 2: Verify it compiles**

Run: `cargo build --release`
Expected: Successful build

**Step 3: Sanity test yield***

Run:
```
./target/release/jsse -e '
function* inner() { yield 1; yield 2; }
function* outer() { var v = yield* inner(); console.log("v=" + v); }
var g = outer();
console.log(JSON.stringify(g.next()));
console.log(JSON.stringify(g.next()));
console.log(JSON.stringify(g.next()));
'
```
Expected: `{"value":1,"done":false}` `{"value":2,"done":false}` `v=undefined` `{"done":true}`

**Step 4: Commit**

```
git add src/interpreter/eval.rs
git commit -m "Fix yield* to use getter-aware iterator_complete/iterator_value"
```

---

### Task 7: Run yield-star test262 tests and measure impact

**Files:** None modified

**Step 1: Run all yield-star async tests**

Run: `uv run python scripts/run-test262.py test262/test/language/statements/class/elements/async-gen-private-method-static/ -j $(nproc)`
Expected: Significant improvement over baseline from Task 1.

**Step 2: Run broader async generator tests**

Run: `uv run python scripts/run-test262.py test262/test/language/expressions/async-generator/ test262/test/language/statements/async-generator/ test262/test/language/expressions/class/ test262/test/language/statements/class/ -j $(nproc)`
Expected: New passes from yield-star tests across multiple directories.

**Step 3: Run for-of and spread tests (also use iterator protocol)**

Run: `uv run python scripts/run-test262.py test262/test/language/statements/for-of/ test262/test/language/expressions/object/ -j $(nproc)`
Expected: Some additional passes from getter-aware iterators.

---

### Task 8: Run full test262 suite and update baselines

**Files:**
- Modify: `README.md` — update pass count
- Modify: `PLAN.md` — update pass count and add entry for this fix
- Generated: `test262-pass.txt` — updated by test runner

**Step 1: Run full test262**

Run: `uv run python scripts/run-test262.py -j $(nproc)`
Expected: Improvement over 83,814 baseline. Zero regressions.

**Step 2: If regressions, investigate and fix**

Check the test runner output for regression count. If any regressions:
- Identify the regressed tests
- Run them individually to diagnose
- Fix the root cause (likely a borrow conflict or a case where `get_property` returned a different shape than `get_object_property`)

**Step 3: Update README.md with new pass count**

Update the test262 progress table in `README.md` with the new numbers.

**Step 4: Update PLAN.md**

Add a new entry (item 60) to the "Recommended Next Tasks" section documenting this fix:
```
60. ~~**Iterator protocol getter-awareness**~~ — Done (+N new passes, X% → Y%).
    Fixed get_iterator, get_async_iterator, iterator_next, iterator_next_with_value,
    iterator_return, iterator_throw, iterator_close to use getter-aware get_object_property.
    Fixed yield* eval path to use iterator_complete/iterator_value.
```

**Step 5: Commit all updates**

```
git add test262-pass.txt README.md PLAN.md
git commit -m "Update test262 baseline after iterator protocol getter-awareness fix"
```

---

### Task 9: Run linter

**Files:** None (fix any lint issues found)

**Step 1: Run lint**

Run: `./scripts/lint.sh`
Expected: Clean pass. If warnings, fix them and commit.
