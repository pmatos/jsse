# Plan: Implement `Iterator.prototype.includes` (issue #549)

## 1. Problem restated

`Iterator.prototype.includes(searchElement [, skippedElements])` does not exist on
jsse's `%IteratorPrototype%`. This is newly-added upstream test262 coverage (test262
bump to `7710052`, #547) for the `iterator-includes` proposal, gated behind the
`iterator-includes` feature flag. It causes 80 failing scenarios across 40 files
under `test262/test/built-ins/Iterator/prototype/includes/`. The method eagerly
consumes a plain (non-Array-like) iterator, optionally skipping a fixed number of
leading elements, and returns whether any subsequent element is `SameValueZero`
to `searchElement`, closing the iterator on a match or on argument-validation
failure.

## 2. Spec basis

`Iterator.prototype.includes` is **not yet merged into `tc39/ecma262`** — confirmed
by grepping both the pinned `spec/spec.html` (submodule at `270a490b`) and
`origin/main` of the `ecma262` repo for `sec-iterator.prototype.includes`: no
match in either. It is a separate, still-unmerged TC39 proposal
(`tc39/proposal-iterator-includes`, copyright header "Michael Ficarra" on every
test file), analogous to `Iterator.prototype.some`/`every`/`find`, which **are**
merged (`sec-iterator.prototype.some` etc. exist in `spec/spec.html`).

Because the proposal text isn't in any repo we're allowed to read as a submodule,
the authoritative algorithm is taken verbatim from the `info:` block that
`test262/test/built-ins/Iterator/prototype/includes/argument-effect-order.js`
(lines 7–25) reproduces from the proposal's own spec, tagged
`esid: sec-iterator.prototype.includes`:

```
Iterator.prototype.includes ( searchElement [ , skippedElements ] )

1. Let O be the this value.
2. If O is not an Object, throw a TypeError exception.
3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
4. If skippedElements is undefined, let toSkip be 0.
5. Else if skippedElements is not one of +Infinity, -Infinity, or an integral Number,
  a. Let error be ThrowCompletion(a newly created TypeError object).
  b. Return ? IteratorClose(iterated, error).
6. Else, let toSkip be skippedElements.
7. If toSkip < -0F, then
  a. Let error be ThrowCompletion(a newly created RangeError object).
  b. Return ? IteratorClose(iterated, error).
8. If toSkip is finite and toSkip > F(2**53 - 1), then
  a. Let error be ThrowCompletion(a newly created RangeError object).
  b. Return ? IteratorClose(iterated, error).
9. Let skipped be +0F.
10. Set iterated to ? GetIteratorDirect(O).
11. Repeat,
  a. Let value be ? IteratorStepValue(iterated).
  b. If value is ~done~, return false.
  c. If skipped < toSkip, set skipped to skipped + 1F.
  d. Else if SameValueZero(value, searchElement) is true, return ? IteratorClose(iterated, NormalCompletion(true)).
```

(Step 11's loop body is reconstructed from the observable behavior pinned by
`skipped-elements-positive-integral.js`, `skipped-elements-positive-infinity.js`,
and `closes-on-match.js` — every one of those tests is consistent with "skip
exactly `toSkip` values, then SameValueZero-compare every value after that,
closing only on a match", never both skipping and comparing the same value.)

The sub-operations this algorithm calls into **are** governed by the pinned
`spec/spec.html` and are cited by clause id:

- `sec-getiteratordirect` — `GetIteratorDirect ( obj )` (spec.html:6987): `? Get(obj, "next")`, builds the Iterator Record. Already implemented in jsse as `get_iterator_direct_getter` (`src/interpreter/builtins/iterators.rs:302`).
- `sec-iteratorstepvalue` — `IteratorStepValue ( iteratorRecord )` (spec.html:7141): calls `[[NextMethod]]`, returns `~done~` or the result's `.value`. Already implemented as the `iterator_step_direct` + `iterator_value` pair (`iterators.rs:4570`, `iterators.rs:4422`), used identically by `some`/`every`/`find`.
- `sec-iteratorclose` — `IteratorClose ( iteratorRecord, completion )` (spec.html:7162): fetches `.return`, calls it if present, original completion wins over a normal `.return()` result but a *throwing* `.return()` overrides a normal completion. Already implemented as `iterator_close_getter` (`iterators.rs:319`).
- `sec-samevaluezero` — `SameValueZero ( x, y )` (spec.html:5919). Already implemented as `same_value_zero` (`src/interpreter/helpers.rs:316`), already used by `Array.prototype.includes` (`src/interpreter/builtins/array.rs:1301`).

No new abstract operations are needed — every sub-operation `includes` calls is
already implemented and exercised by sibling `Iterator.prototype` methods. The
only genuinely new logic is the top-level algorithm's own argument validation
(steps 4–8) and the skip-then-compare loop (step 11).

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs` — add the `includes` method to
  `%IteratorPrototype%` inside the same setup function that registers `some`,
  `every`, `find` (around `iterators.rs:1502–1653`). Insert it as a new
  `self.define_method(iter_proto_id, "includes", 1, ...)` block, placed after
  `find` (line ~1653) and before `reduce`, grouping it with the other
  eager "search" methods.
- No other `src/` files change: no new `ObjectKind` variant, no new GC roots, no
  parser/lexer changes (this is a builtin method, not new syntax).
- `test262-extra/` — one new file for the ordering case test262 doesn't cover
  (see §5).
- No `docs/adr/` entry: this is an additive builtin implemented entirely with
  existing infrastructure (`get_iterator_direct_getter`, `iterator_step_direct`,
  `iterator_value`, `iterator_close_getter`, `same_value_zero`), not an
  architectural decision.
- No `CONTEXT.md` change: no new domain vocabulary.
- `README.md` — update test262 pass count/percentage after the full suite run,
  per repo convention.

## 4. TDD slices

Each slice is red (targeted test262 run shows failures) → green (implement) →
narrow re-run. All slices land in one `define_method` block since the method is
one self-contained closure; "slices" here are the algorithm steps built up
incrementally and checked against specific test262 files as each is reached.

1. **This-value and predicate-shape validation.**
   Tests: `this-non-object.js`, `this-non-callable-next.js`, `callable.js`,
   `is-function.js`, `length.js`, `name.js`, `prop-desc.js`, `proto.js`,
   `non-constructible.js`.
   Production: register `includes` via `define_method(iter_proto_id, "includes", 1, ...)`
   with an explicit `if !this.is_object() { throw TypeError }` first step —
   **do not** copy the `some`/`every`/`find` pattern of skipping this check and
   relying on `get_iterator_direct_getter` to fail later (see step 3 rationale
   in §6). This mirrors `take`/`drop`'s explicit check at `iterators.rs:2005`
   and `iterators.rs:2166`.

2. **`skippedElements` argument validation, order-sensitive.**
   Tests: `argument-effect-order.js`, `argument-validation-failure-closes-underlying.js`,
   `skipped-elements-not-a-number.js`, `skipped-elements-no-coercion.js`,
   `skipped-elements-non-integral-typeerror.js`, `skipped-elements-nan-typeerror.js`,
   `skipped-elements-negative-integral-rangeerror.js`,
   `skipped-elements-negative-infinity-rangeerror.js`,
   `skipped-elements-too-large-rangeerror.js`, `skipped-elements-max-safe-integer.js`,
   `skipped-elements-zero-and-negative-zero.js`, `skipped-elements-default.js`.
   Production: after the `this.is_object()` check, read `args.get(1)`. If
   `undefined` (or absent), `to_skip = 0.0`. Else require `arg.as_number()` to be
   `Some(n)` (no `ToNumber` coercion — `as_number()` only returns `Some` for an
   actual Number-typed `JsValue`, matching `skipped-elements-no-coercion.js`'s
   requirement that `valueOf`/`toString` are never called) **and**
   `n.is_infinite() || n.trunc() == n` (use `trunc()`, not `fract() == 0.0` —
   `f64::INFINITY.fract()` is `NaN`, which would wrongly reject `+Infinity`).
   Any other case (non-Number, or a Number failing the integral/infinite check)
   throws `TypeError` via `iterator_close_getter(interp, this)` (original-error-wins
   idiom, `let _ = iterator_close_getter(...)`, matching `some`'s
   line 1510 and `take`'s line 2016/2022/2029 — **not** the propagating idiom used
   on the match path). Then range-check: `to_skip < 0.0` → RangeError (same
   `let _ = iterator_close_getter` idiom); `to_skip.is_finite() && to_skip > 2f64.powi(53) - 1.0`
   → RangeError (same idiom). Only after all validation succeeds, call
   `get_iterator_direct_getter(interp, this)` to fetch `next` (this ordering is
   what `argument-effect-order.js` pins: the `next` getter must not fire until
   `skippedElements` validation has passed).

3. **Skip-then-compare loop, natural exhaustion, `SameValueZero`.**
   Tests: `basic-match-and-miss.js`, `skipped-elements-positive-integral.js`,
   `skipped-elements-positive-infinity.js`, `samevaluezero-nan.js`,
   `samevaluezero-zeroes.js`, `object-identity.js`, `symbol-identity.js`,
   `infinite-iterator.js`, `iterator-already-exhausted.js`,
   `exhaustion-does-not-call-return.js`, `get-next-method-only-once.js`,
   `get-next-method-throws.js`, `next-method-throws.js`,
   `next-method-returns-non-object.js`, `next-method-returns-throwing-done.js`,
   `next-method-returns-throwing-value-done.js`, `next-method-returns-throwing-value.js`,
   `result-is-boolean.js`, `this-plain-iterator.js`.
   Production: `let mut skipped = 0.0f64;` then loop on
   `interp.iterator_step_direct(&iter, &next_method)` exactly like `some`
   (`iterators.rs:1519–1550`): `Ok(None)` → `return Completion::Normal(JsValue::FALSE)`
   (no `.return()` call — matches `exhaustion-does-not-call-return.js`);
   `Err(e)` → `return Completion::Throw(e)` (propagates `next`/`.value`-getter
   throws without attempting to close, matching `next-method-throws.js` and
   `next-method-returns-throwing-value.js`, consistent with how `some` handles
   the same `Err` arm); `Ok(Some(result))` → `iterator_value(&result)` to get
   `value`, then: if `skipped < to_skip`, increment `skipped` and continue; else
   compare `same_value_zero(&value, &search_element)` — no match, continue; match,
   close and return.

4. **Close-on-match, propagating a throwing `.return()`.**
   Tests: `closes-on-match.js`, `get-return-method-throws.js`,
   `iterator-return-method-throws.js`, `iterator-has-no-return.js`.
   Production: on match, use the *propagating* idiom (mirrors `some`'s
   lines 1533–1536): `if let Err(e) = iterator_close_getter(interp, &iter) { return Completion::Throw(e); } return Completion::Normal(JsValue::TRUE);` —
   this is deliberately the opposite of the `let _ = ...` idiom used for
   argument-validation failures in slice 2, because step 11.d is
   `Return ? IteratorClose(iterated, NormalCompletion(true))`: the `?` means a
   throwing `.return()` must override the `true` result, whereas in the
   validation-failure paths the *original* TypeError/RangeError always wins over
   whatever `.return()` does (`IteratorClose`'s own steps 7181–7182: "If
   completion is a throw completion, return ? completion" — checked *before*
   the inner-result throw check).

5. **Full-directory regression pass.** Run the whole
   `test262/test/built-ins/Iterator/prototype/includes/` directory and confirm
   all 40 files/80 scenarios pass, then run the full test262 suite to confirm no
   unrelated regressions.

## 5. Test surface

- Targeted: `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/includes/`
  — all 40 files, 80 scenarios, must go green. No feature skip-list exists in
  `scripts/run-test262.py` (confirmed: only a `FEATURES_RE` parser, no
  allow/deny filtering), so `iterator-includes`-tagged tests run by default.
- Full regression: `uv run python scripts/run-test262.py` (default `language/`,
  `built-ins/`, `annexB/`, `intl402/`) to confirm no baseline regressions
  (baseline read from `origin/main:test262-pass.txt`, not rewritten by this
  branch).
- Gap not covered by test262: the ordering between step 2 (`this` non-object →
  TypeError) and steps 4–8 (`skippedElements` validation → TypeError/RangeError).
  Every existing `this`-non-object test (`this-non-object.js`) only ever passes
  a valid-or-absent `skippedElements`, so it can't distinguish "check `this`
  first" from "validate `skippedElements` first, and the no-op `iterator_close_getter`
  on a non-object `this` happens to also produce the right error type by
  accident." A new `test262-extra/built-ins/Iterator/prototype/includes/this-non-object-precedes-skipped-elements-validation.js`
  test should call `Iterator.prototype.includes.call(null, 0, Number.MAX_SAFE_INTEGER + 1)`
  (a value that would raise `RangeError` if `skippedElements` were checked
  first) and assert it throws `TypeError`, not `RangeError`, following the
  existing test262 frontmatter pattern (`esid: sec-iterator.prototype.includes`,
  `features: [iterator-includes]`, an `info:` block quoting spec steps 1–2).
- `cargo test --release` (per CLAUDE.md: crate is bin-only, use `--bin jsse`) —
  run as part of the standard quality gate; no new Rust unit tests are planned
  since test262 + test262-extra already exercise every branch above.

## 6. Regression risk

- **Low risk to shared machinery.** `includes` reuses `get_iterator_direct_getter`,
  `iterator_step_direct`, `iterator_value`, `iterator_close_getter`, and
  `same_value_zero` verbatim — all four are already exercised by `some`/`every`/
  `find`/`take`/`drop`/`Array.prototype.includes`. No changes to those shared
  functions are planned, so this cannot move behavior for any other builtin.
- **The one real risk is copying the wrong precedent.** `some`/`every`/`find`
  skip the explicit `this.is_object()` check (relying on `get_iterator_direct_getter`
  to fail later); `includes` cannot follow that shortcut because argument
  validation happens *before* `GetIteratorDirect` is called, so a naive port
  would report `RangeError` instead of `TypeError` for non-object `this` with an
  out-of-range `skippedElements`. Mitigated by slice 1's explicit check and the
  `test262-extra` case in §5.
- **No interaction with**: the tree-walker hot paths (`eval_expr`/`exec_statement`
  are untouched — this is a native builtin), the property MOP in `property.rs`
  (no new exotic behavior), GC rooting (no new `ObjectKind` variant, no
  cross-tick closure state the way `take`/`drop`/`map`/`filter` need — `includes`
  is eager and single-call, following exactly the no-explicit-rooting pattern
  already used by `some`/`every`/`find`/`reduce`/`forEach`/`toArray`), the
  exhaustive `ObjectKind` match (no new variant added), the bytecode fast path
  (Iterator Helper builtins are not compiled, only interpreted — consistent with
  every existing method here), or the Node-compat library harnesses (no library
  under test currently calls `Iterator.prototype.includes`).
- **Baseline**: `test262-pass.txt` is not touched; the runner reads it from
  `origin/main` per repo convention, so this branch only needs to *increase*
  pass count without regressing anything already green.

## 7. Out of scope

- No refactor of `some`/`every`/`find` to share a common "eager predicate loop"
  helper, even though `includes` is structurally close to them. Three-plus
  similar closures is not yet enough to justify an abstraction, and unifying
  them now would bundle an unrelated refactor into this bug-shaped PR.
- No change to `Array.prototype.includes` or `TypedArray.prototype.includes`
  (`src/interpreter/builtins/array.rs:1269`, `typedarray.rs`) even though they
  share `same_value_zero` — they are unaffected and out of scope.
- No attempt to reconcile jsse's `spec/` submodule pin with the fact that this
  proposal isn't merged upstream — that's an ecma262/test262 upstream state, not
  something jsse's `spec/` submodule can or should paper over.
- No update to `test262-pass.txt` (main-branch-only operation, per repo
  convention).
