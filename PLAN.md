# Plan: issue #826 — `Iterator.prototype.toArray` loses thrown value's identity under GC stress

## 1. Problem restated

`Iterator.prototype.toArray`'s next-method loop
(`src/interpreter/builtins/iterators.rs:1606-1640`, the `"toArray"` closure
registered in `setup_iterator_helper_methods`) calls
`iterator_close_getter(interp, &iter)` — which invokes the underlying
iterator's `return()` method and can therefore run arbitrary user script — on
*both* of its error paths: when `iterator_step_direct` fails (line 1632-1635:
the `next()` call or its `done` getter threw) and when `iterator_value` fails
(line 1623-1626: the result's `value` getter threw). In both branches the
already-produced thrown value `e` is **not GC-rooted** before that call. Under
`JSSE_GC_STRESS` (confirmed live: `JSSE_GC_STRESS=2 uv run python
scripts/run-test262.py test262/test/built-ins/Iterator/prototype/toArray/next-method-returns-throwing-value.js`
fails 1 of 2 scenarios, reproducibly, across repeated runs), a collection
triggered while `return()` runs can free `e`'s arena slot; a later allocation
inside `return()` (or its own thrown error) can land on that same recycled
slot, so `e` reads back as a different, live-looking object instead of the
original one. A local repro without the stress harness (`return()` calling
`$262.gc()` directly) shows this precisely: the propagated error becomes
`return()`'s own `RangeError`, not the original `TypeError` from the `value`
getter. Depending on allocation order this can also surface as the "reads back
as `undefined`" symptom the issue reports from the nightly sampled run. The
fix is not "root `e` across the close" — it is that `toArray` should not be
calling `iterator_close_getter` on these two paths at all, per the current
spec text (§2); removing the two misplaced calls both restores spec
conformance and eliminates the GC-unsafe window outright.

While tracing this closure a second, independent missing-root bug was found in
the *success* path of the same loop (§4 slice 5, §7): `values.push(v)` at line
1622 pushes the extracted element into a plain `Vec<JsValue>` without rooting
`v` first. A `Vec<JsValue>` is not itself scanned by the collector — only
`gc_temp_roots` is — so an element accumulated on one iteration can be freed
by a GC triggered during a *later* iteration's `next()` call, silently
corrupting the array `toArray()` eventually returns. This reproduces 100% on
the current release binary with a plain `$262.gc()` in `next()`, no stress
flag needed (see §4 slice 5). It is in the same closure, is the same class of
fix (root it like the sibling `iterate_to_vec`, `src/interpreter/builtins/iterators.rs:5087-5107`,
already does), and is included as an additional slice in this same PR rather
than deferred.

## 2. Spec basis

- **`sec-iteratorstepvalue`** — *IteratorStepValue ( iteratorRecord )* (`spec/spec.html:7141-7160`):
  ```
  1. Let result be ? IteratorStep(iteratorRecord).
  2. If result is done, then
    a. Return done.
  3. Let value be Completion(IteratorValue(result)).
  4. If value is a throw completion, then
    a. Set iteratorRecord.[[Done]] to true.
  5. Return ? value.
  ```
  Step 1 uses a bare `?` on `IteratorStep` (next() throwing, or its `done`
  getter throwing) — straight propagation, no `IteratorClose`. Steps 3-5: if
  the `value` getter throws, the record is marked `[[Done]]` and the
  completion is propagated with another bare `?` — again no `IteratorClose`,
  no call to `return()`.
- **`sec-iterator.prototype.toarray`** — *Iterator.prototype.toArray ( )* (`spec/spec.html:48664-48677`):
  ```
  1. Let O be the this value.
  2. If O is not an Object, throw a TypeError exception.
  3. Let iterated be ? GetIteratorDirect(O).
  4. Let items be a new empty List.
  5. Repeat,
    a. Let value be ? IteratorStepValue(iterated).
    b. If value is done, return CreateArrayFromList(items).
    c. Append value to items.
  ```
  `toArray` wraps the `IteratorStepValue` call in a bare `?` too — it adds no
  `IfAbruptCloseIterator` of its own. Across both clauses there is **no spec
  path** in which `toArray` calls the iterator's `return()` method when
  `next()`, its `done` getter, or its `value` getter throws.
- Contrast with `forEach`/`map`/`reduce`/etc. (`spec/spec.html:48532-48610`),
  which *do* wrap the user-supplied callback's own call in
  `IfAbruptCloseIterator` — a different step (calling the callback, not
  `IteratorStepValue`). jsse's implementations of those methods already omit
  closing on the `IteratorStepValue` failure path, matching spec: compare
  `toArray`'s loop (`iterators.rs:1616`, `Err(e) =>` arms at 1623-1626 and
  1632-1635) against `forEach`'s (`iterators.rs:1655`, bare `Err(e) => break
  Completion::Throw(e)` at 1662 and 1675, no close), `some`'s (`iterators.rs:1695`,
  bare at 1702 and 1726), `every`'s (`iterators.rs:1746`, bare at 1753 and
  1776), `find`'s (`iterators.rs:1797`, bare at 1804 and 1826), and `reduce`'s
  (`iterators.rs:1946` and `1967`, bare at 1953/1963 and 1974). `toArray` is
  the sole outlier calling `iterator_close_getter` on this path; these five are
  the existing, correct precedent to match.
- The `values`-rooting fix (§1, §4 slice 5) has no separate spec clause of its
  own to cite beyond `CreateArrayFromList` (`sec-iterator.prototype.toarray`
  step 5.b/5.c, "Append value to items" then "return
  CreateArrayFromList(items)") — it is an engine GC-rooting defect, not a
  semantics gap: `items`/`values` must still contain the exact values the
  spec's `Append` steps put there by the time `CreateArrayFromList` runs.

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs`, inside `setup_iterator_helper_methods`'s
  `"toArray"` closure (`iterators.rs:1606-1640`):
  - Remove the two `let _ = iterator_close_getter(interp, &iter);` statements
    (the `Err(e)` arm of the `iterator_step_direct` match at line 1632-1635,
    and the `Err(e)` arm of the `iterator_value` match at line 1623-1626),
    leaving bare `Err(e) => break Completion::Throw(e),` in both places — the
    shape already used by `forEach`/`some`/`every`/`find`/`reduce`.
  - At line 1622, root the extracted value before accumulating it:
    `interp.gc_root_value(&v); values.push(v);`, mirroring `iterate_to_vec`
    (`iterators.rs:5087-5107`). The existing `gc_unroot_frame(frame)` at the
    end of the closure (line 1638) already releases every value pushed this
    way in bulk, so no per-element unroot is needed.
- `test262-extra/Iterator-prototype-toArray-return-not-called-gc-rooting.js`
  (new) — regression for the `iterator_close_getter` removal, content in §5.
- `test262-extra/Iterator-prototype-toArray-accumulated-value-gc-rooting.js`
  (new) — regression for the `values` rooting fix, content in §5.

No `docs/adr/` entry: two bug fixes inside one existing, already-documented
closure, not a new architectural decision. No `CONTEXT.md` change: no new
vocabulary introduced.

## 4. TDD slices

1. **Red (close-call removal)**: add
   `test262-extra/Iterator-prototype-toArray-return-not-called-gc-rooting.js`
   (§5) and run it against the current release binary. Confirm it fails today
   on at least one of its four cases — `returnCalled` becomes `true` for the
   `next()`-throws, `done`-getter-throws, and `value`-getter-throws cases
   (since `iterator_close_getter` is currently invoked on all three), and the
   `value`-getter-throws case's identity assertion fails under the `$262.gc()`
   + allocation churn placed inside `return()`.
2. **Confirm against the issue's own test262 test**: run
   `JSSE_GC_STRESS=2 uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/toArray/next-method-returns-throwing-value.js`
   and confirm it still fails (already reproduced live during planning: 1 of 2
   scenarios fails, reproducibly across repeated runs).
3. **Green (close-call removal)**: in `iterators.rs`, delete the two
   `let _ = iterator_close_getter(interp, &iter);` lines inside `toArray`'s
   loop (§3). Rebuild (`cargo build --release`) and re-run both the new
   test262-extra file (must pass, all four cases) and the command from slice 2
   (must now pass both scenarios, no `JSSE_GC_STRESS` needed to see the
   difference but run it with `JSSE_GC_STRESS=2` anyway since that is the
   issue's own reproduction command).
4. **Red (values-rooting)**: add
   `test262-extra/Iterator-prototype-toArray-accumulated-value-gc-rooting.js`
   (§5) and run it against the binary from slice 3 (close-call fix applied,
   values-rooting fix not yet applied). Confirm it fails: a `next()` that
   calls `$262.gc()` plus allocation churn before returning each fresh
   `{tag: i}` value corrupts earlier-pushed array elements by the time
   `toArray()` returns (reproduced live during planning on the pre-fix binary:
   expected `[{tag:0},{tag:1},{tag:2},{tag:3},{tag:4}]`, got
   `[{}, {done:true}, {done:true}, {done:true}, {done:true}]` — no
   `JSSE_GC_STRESS` flag needed, a single `$262.gc()` is deterministic here).
5. **Green (values-rooting)**: add `interp.gc_root_value(&v);` immediately
   before `values.push(v)` at `iterators.rs:1622` (§3). Rebuild and re-run the
   slice-4 test; it must now report the correct five tagged elements.
6. **No-regression check, targeted**: run
   `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/`
   (the whole `Iterator/` subtree, not just `toArray/` — `.toArray()` is used
   as a terminal/consumption step by other iterator-helper tests, e.g. for
   helpers chained off `.drop()`/`.take()`/`.map()`/`.filter()`, so the
   regression surface is wider than the one directory). Confirm zero
   regressions against the baseline (`origin/main:test262-pass.txt`).
7. **No-regression check, GC stress**: run both new test262-extra files, and
   the `test262/test/built-ins/Iterator/` subtree, under
   `JSSE_GC_STRESS=1,2,4,8,16`:
   `for n in 1 2 4 8 16; do JSSE_GC_STRESS=$n uv run python scripts/run-test262.py test262-extra/ test262/test/built-ins/Iterator/ || echo "FAIL at N=$n"; done`
   Also run the two new test262-extra files with `--bytecode` and with
   `--binary target/release-checked/jsse` (both axes CI exercises for
   test262-extra per `CLAUDE.md`'s GC Stress Mode / Root-Stack Discipline
   sections).
8. **Refactor**: none planned. The two fixes are a two-line deletion and a
   one-line addition; there is nothing left to clean up in the touched
   closure. (Not pursuing: collapsing `toArray`'s loop with the now-identical
   shape of `forEach`/`some`/`every`/`find`'s — out of scope, §7.)

## 5. Test surface

- **Targeted test262**: `test262/test/built-ins/Iterator/` (widened from just
  `toArray/` per slice 6) via
  `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/`.
  The `toArray/` subdirectory alone is 18 files / 36 scenarios, confirmed
  36/36 passing on the current baseline before this change.
- **The issue's own reproduction**, run directly as a non-regression check
  (slice 2/3):
  `JSSE_GC_STRESS=2 uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/toArray/next-method-returns-throwing-value.js`.
- **New `test262-extra/` regressions** (not expressible in test262 proper:
  test262 has no portable way to force a GC at a specific point, and the
  existing `toArray` tests only check the propagated error's *type* via
  `assert.throws`, never whether `return()` was invoked or whether the error
  is the *same object*; nor is there a test262 way to assert on accumulated
  intermediate values surviving GC). Both follow the
  `RegExp-split-splitter-construction-gc-rooting.js` /
  `Map-constructor-under-construction-gc-rooting.js` precedent (`features:
  [host-gc-required]`, explicit `$262.gc()` plus allocation churn to force
  reuse of a freed arena slot).

  **`test262-extra/Iterator-prototype-toArray-return-not-called-gc-rooting.js`** sketch:
  ```js
  /*---
  esid: sec-iterator.prototype.toarray
  description: >
    IteratorStepValue never performs IteratorClose, whether IteratorStep
    throws (next() or its done getter) or IteratorValue throws (the value
    getter) -- so Iterator.prototype.toArray must never invoke the
    underlying iterator's return() on any of these paths, and the
    propagated error must retain its original identity across whatever
    garbage collection the engine performs while completing that step.
  info: |
    %Iterator.prototype%.toArray ( )
    5. Repeat,
      a. Let value be ? IteratorStepValue(iterated).
    IteratorStepValue ( iteratorRecord )
    1. Let result be ? IteratorStep(iteratorRecord).
    3. Let value be Completion(IteratorValue(result)).
    4. If value is a throw completion, then
      a. Set iteratorRecord.[[Done]] to true.
    5. Return ? value.
  features: [host-gc-required]
  ---*/
  class Marker extends Error {}

  function makeIterator(nextImpl) {
    var state = { returnCalled: false };
    class T extends Iterator {
      next() { return nextImpl(); }
      return() {
        state.returnCalled = true;
        // The close, if it ran at all, is exactly where arbitrary script
        // (and therefore a GC) must not be able to disturb the error
        // already produced by IteratorStepValue.
        $262.gc();
        for (var i = 0; i < 64; i++) { [{}, {}, {}]; }
        throw new Error("return() must not be called here");
      }
    }
    return { iterator: new T(), state };
  }

  function run(nextImpl) {
    var { iterator, state } = makeIterator(nextImpl);
    var thrown;
    try {
      iterator.toArray();
    } catch (e) {
      thrown = e;
    }
    return { returnCalled: state.returnCalled, thrown };
  }

  // Case 1: next() itself throws.
  var r1 = run(function () { throw new Marker("next"); });
  assert.sameValue(r1.returnCalled, false, "next()-throws: must not call return()");
  assert(r1.thrown instanceof Marker, "next()-throws: must propagate the original error");

  // Case 2: the done getter throws.
  var r2 = run(function () {
    return { get done() { throw new Marker("done"); }, value: 1 };
  });
  assert.sameValue(r2.returnCalled, false, "done-getter-throws: must not call return()");
  assert(r2.thrown instanceof Marker, "done-getter-throws: must propagate the original error");

  // Case 3: the value getter throws (the issue's exact scenario).
  var r3 = run(function () {
    return { done: false, get value() { throw new Marker("value"); } };
  });
  assert.sameValue(r3.returnCalled, false, "value-getter-throws: must not call return()");
  assert(r3.thrown instanceof Marker, "value-getter-throws: must propagate the original error");

  // Case 4: next() returns a non-object (no user value to lose, but return()
  // must still not be called).
  var r4 = run(function () { return null; });
  assert.sameValue(r4.returnCalled, false, "non-object-result: must not call return()");
  assert(r4.thrown instanceof TypeError, "non-object-result: must throw a TypeError");
  ```

  **`test262-extra/Iterator-prototype-toArray-accumulated-value-gc-rooting.js`** sketch:
  ```js
  /*---
  esid: sec-iterator.prototype.toarray
  description: >
    Each value Iterator.prototype.toArray appends to its result list (step
    5.c, "Append value to items") must stay reachable across every later
    iteration of the Repeat loop -- a later next() call can run arbitrary
    script, and therefore trigger a garbage collection, before
    CreateArrayFromList ever runs.
  info: |
    %Iterator.prototype%.toArray ( )
    5. Repeat,
      a. Let value be ? IteratorStepValue(iterated).
      b. If value is done, return CreateArrayFromList(items).
      c. Append value to items.
  features: [host-gc-required]
  ---*/
  var i = 0;
  class ChurnIterator extends Iterator {
    next() {
      $262.gc();
      for (var j = 0; j < 200; j++) { [{}, {}, {}, {}]; }
      if (i >= 5) return { done: true, value: undefined };
      var v = { tag: i };
      i++;
      return { done: false, value: v };
    }
  }

  var arr = new ChurnIterator().toArray();
  assert.sameValue(arr.length, 5);
  for (var k = 0; k < 5; k++) {
    assert.sameValue(arr[k].tag, k, "element " + k + " lost its identity");
  }
  ```

  Run both directly (`./target/release/jsse test262-extra/<file>.js`) and
  through the runner (`uv run python scripts/run-test262.py test262-extra/`),
  normal, `--bytecode`, under `JSSE_GC_STRESS=1..16`, and on
  `target/release-checked/jsse`, per TDD slice 7.
- **Full gate**: `cargo test --release` (no new Rust unit test is planned
  since both defects are only observable at the JS-semantics level
  `test262-extra` already covers), plus a full
  `uv run python scripts/run-test262.py` pass to confirm zero regressions
  against the baseline (read from `origin/main:test262-pass.txt`) — mandatory,
  not time-permitting, per this project's "run the full test262 suite after
  any implementation work" rule.

## 6. Regression risk

- **`test262-pass.txt` baseline**: low risk for the close-call removal — it
  only removes `iterator_close_getter` calls on two `toArray`-internal error
  paths and does not touch `iterator_close_getter`, `iterator_value`,
  `iterator_step_direct`, or `close_iterator_for_error` themselves, so no
  other `%IteratorPrototype%` helper changes behavior. Low risk for the
  values-rooting fix — it only adds a root, never removes one, and roots are
  released in the same bulk `gc_unroot_frame` as before, so it cannot affect
  any passing test's *correct* output, only prevent incorrect corruption.
  Verified empirically (slice 6) against the widened `Iterator/` subtree,
  since `.toArray()` is a common terminator for other iterator-helper tests
  and the regression surface is wider than `toArray/` alone.
- **Shared machinery leaned on**: `gc_root_frame`/`gc_root_value`/
  `gc_unroot_value` bracketing in the `toArray` closure gains one more
  balanced root/bulk-release pair; the LIFO root-stack discipline
  (`CLAUDE.md`'s GC Root-Stack Discipline section) is preserved because the
  new root is never individually popped — it rides out to the existing
  `gc_unroot_frame(frame)` at the end, identical to `iterate_to_vec`'s
  pattern. `ObjectKind`'s exhaustive match in `gc::trace_object_fields` is
  untouched (no new object kind). The bytecode compiler/VM is untouched:
  `toArray` is a native Rust closure registered via `define_method`, not an
  AST body the bytecode compiler ever sees.
- **Net effect on GC safety**: the close-call removal *shrinks* the GC-unsafe
  window in this closure; the values-rooting addition closes a previously
  completely unprotected window. Neither changes the rooting contract any
  other closure relies on.

## 7. Out of scope

- **Same close-call bug class, different call sites, same file.** A grep of
  `iterator_close_getter` in `iterators.rs` turns up more call sites with the
  identical *unrooted-value-across-an-arbitrary-script-call* shape, but where
  closing itself *is* spec-mandated (unlike `toArray`'s two removed calls), so
  the correct fix there is "root the value across the close via the existing
  `close_iterator_for_error` helper" (already used correctly at
  `iterators.rs:2342` and its siblings in the `drop`/`take` argument
  prologue), not "stop closing":
  - `forEach`'s callback-throw branch, `iterators.rs:1664-1671` — `e` from
    `interp.call_function(&callback, ...)` is not rooted before the
    `iterator_close_getter` call at line 1669.
  - `reduce`'s reducer-throw branch, `iterators.rs:1982-1985` — same shape.
  - `find`'s "found" branch, `iterators.rs:1811-1815` — the found `value`
    (not an error this time, but still a value that must survive) is not
    rooted before the `iterator_close_getter` call at line 1812, between
    `call_function` returning and `break Completion::Normal(value)`.
  This is a different failure mode than #826's repro exercises and a
  different fix shape (root, don't delete) from this PR's two slices.
  Recommend filing a follow-up issue referencing this plan and auditing the
  rest of `iterators.rs`'s `iterator_close_getter` call sites for the same
  pattern, rather than bundling it here.
- No changes to `iterator_close_getter`, `iterator_value`,
  `iterator_step_direct`, or `close_iterator_for_error` themselves.
- No `test262-pass.txt` baseline update (main-branch-only operation).
- No refactor collapsing `toArray`'s loop with the now even more textually
  similar shape of `forEach`/`some`/`every`/`find`'s — out of scope for a bug
  fix.
