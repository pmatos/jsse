# Plan: issue #569 — iterator helpers construct validation errors after IteratorClose

## 1. Problem restated

Ten `Iterator.prototype` helpers (`forEach`, `some`, `every`, `find`, `reduce`,
`map`, `filter`, `take`, `drop`, `flatMap`) validate an argument, and on
failure must construct a `TypeError`/`RangeError` and then run
`IteratorClose` on the (possibly not-yet-`GetIteratorDirect`'d) receiver. jsse
currently runs `IteratorClose` *first* and constructs the error *after*, at 14
call sites across these 10 helpers. `IteratorClose` invokes the iterator's
user-controllable `return()` method, and jsse's `create_error`/
`create_type_error` resolve the error constructor dynamically via
`get_global_var`. So a `return()` that reassigns the global `TypeError` or
`RangeError` changes the prototype of the error jsse ends up throwing — a
divergence from the spec, which always creates the error object before
`IteratorClose` runs. `Iterator.prototype.includes` had the same defect and
was already fixed in PR #558; this issue is the mechanical follow-up for the
remaining 14 sites.

## 2. Spec basis

Pinned submodule commit `spec/spec.html` at `270a490b3f8bf6f15bced16021ee0c3ff107f823`:

- `sec-iterator.prototype.foreach`, `sec-iterator.prototype.some`,
  `sec-iterator.prototype.every`, `sec-iterator.prototype.find`,
  `sec-iterator.prototype.reduce`, `sec-iterator.prototype.map`,
  `sec-iterator.prototype.filter`, `sec-iterator.prototype.flatmap` — each
  begins: "If IsCallable(_x_) is *false*, then 1. Let _error_ be
  ThrowCompletion(a newly created *TypeError* object). 1. Return ?
  IteratorClose(_iterated_, _error_)." At this point `_iterated_` is still
  `{ [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }` —
  `GetIteratorDirect` has not run yet — so `IteratorClose` operates on `O`
  itself (`this` in the jsse code), matching the existing call sites that
  pass `this`, not a post-`GetIteratorDirect` `iter`.
- `sec-iterator.prototype.take`, `sec-iterator.prototype.drop` — after
  `ToNumber`: "If numLimit is NaN, then ... ThrowCompletion(a newly created
  RangeError object) ... Return ? IteratorClose(iterated, error)." and "If
  integerLimit < 0, then ..." same shape. Both checks also run before
  `GetIteratorDirect`, so `IteratorClose` again targets `this`.
- `sec-iteratorclose` step 5: "If completion.[[Type]] is throw, return ?
  completion" — even when `.return()` itself throws or returns something
  unusable, a pre-existing throw completion is what propagates. This is why
  jsse's `iterator_close_getter`'s own `Err` can be discarded (`let _ =`) at
  these validation sites: the error already captured before the call is the
  one that must be thrown, not whatever `.return()` produces.
- `sec-ifabruptcloseiterator` — the general "construct-error-then-close"
  macro used throughout the algorithms above; it never runs `IteratorClose`
  before the abrupt completion it is closing over has been created.

One wrinkle, not part of this issue: jsse's `take`/`drop` implement a third
RangeError check (`limit` finite and `> 2**53 - 1`) that does not appear in
the pinned `spec.html` text for either clause. It **is** required by
test262's `test262/test/built-ins/Iterator/prototype/{take,drop}/limit-rangeerror.js`,
which cites it as spec step 7/8 of an earlier revision. The check itself is
not touched by this issue — the fix reorders whichever of the three RangeError
branches fires, without adding, removing, or re-justifying any of them.

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs` — the 14 call sites, plus one new
  private helper function colocated with `iterator_close_getter` (~line 319)
  and `iterator_close_with_completion` (~line 524).
- `test262-extra/built-ins/Iterator/prototype/{forEach,some,every,find,reduce,map,filter,take,drop,flatMap}/`
  — one new test file per directory (10 new files, all 10 directories are
  new; `test262-extra/built-ins/Iterator/prototype/includes/` already has the
  pattern file to follow).

No `docs/adr/` entry: this is a bug fix restoring already-decided spec
behavior, not a new architectural decision. No `CONTEXT.md` change: no new
vocabulary.

## 4. Implementation approach

Add one helper next to `iterator_close_getter`:

```rust
// Closes `iterator` while preserving a pending validation error, mirroring
// the includes() fix (PR #558): the error is constructed first and rooted
// across the close, so a return() that swaps the global error constructor
// cannot change its prototype, and a return()-triggered GC cannot reclaim it.
fn close_iterator_for_error(interp: &mut Interpreter, iterator: &JsValue, err: JsValue) -> JsValue {
    interp.gc_root_value(&err);
    let _ = iterator_close_getter(interp, iterator);
    interp.gc_unroot_value(&err);
    err
}
```

Each of the 14 sites changes from:

```rust
let _ = iterator_close_getter(interp, this);
let err = interp.create_type_error("...");
return Completion::Throw(err);
```

to:

```rust
let err = interp.create_type_error("...");
let err = close_iterator_for_error(interp, this, err);
return Completion::Throw(err);
```

This is the one-statement reorder the issue describes, generalized into a
shared helper (per the issue's own suggestion) so the correct order is the
only order the call sites can express. All 14 sites close `this` (never a
post-`GetIteratorDirect` `iter`), so the helper's signature only needs to
take the receiver value, matching every site.

## 5. TDD slices

Each slice: write the failing test262-extra file first, run it against the
unpatched build to confirm it's red (thrown error's prototype is the
`return()`-installed fake, not the original), then apply the reorder at that
helper's site(s) and confirm green.

1. **forEach** (1 site, `iterators.rs:1480`) — add
   `test262-extra/built-ins/Iterator/prototype/forEach/error-created-before-close.js`.
   Introduce `close_iterator_for_error` in this slice (first caller), fix the
   `forEach` site.
2. **some** (1 site, `:1522`) — add
   `test262-extra/built-ins/Iterator/prototype/some/error-created-before-close.js`,
   fix the site.
3. **every** (1 site, `:1574`) — add
   `test262-extra/built-ins/Iterator/prototype/every/error-created-before-close.js`,
   fix the site.
4. **find** (1 site, `:1625`) — add
   `test262-extra/built-ins/Iterator/prototype/find/error-created-before-close.js`,
   fix the site.
5. **reduce** (1 site, `:1771`) — add
   `test262-extra/built-ins/Iterator/prototype/reduce/error-created-before-close.js`,
   fix the site. (The separate "empty iterator, no initial value" TypeError a
   few lines below is a plain throw per spec, not an `IteratorClose` site —
   leave it untouched.)
6. **map** (1 site, `:1912`) — add
   `test262-extra/built-ins/Iterator/prototype/map/error-created-before-close.js`,
   fix the site.
7. **filter** (1 site, `:2043`) — add
   `test262-extra/built-ins/Iterator/prototype/filter/error-created-before-close.js`,
   fix the site.
8. **take** (3 sites: NaN `:2190`, too-large `:2197`, negative `:2211`) —
   one file,
   `test262-extra/built-ins/Iterator/prototype/take/limit-error-created-before-close.js`,
   exercising all three cases (mirrors `includes/skipped-elements-error-created-before-close.js`'s
   single-file, multi-case layout); fix all three sites together since they
   share one helper function body.
9. **drop** (3 sites: NaN `:2350`, too-large `:2357`, negative `:2371`) —
   one file,
   `test262-extra/built-ins/Iterator/prototype/drop/limit-error-created-before-close.js`,
   same shape as take's; fix all three sites.
10. **flatMap** (1 site, `:2806`) — add
    `test262-extra/built-ins/Iterator/prototype/flatMap/error-created-before-close.js`,
    fix the site.

(Line numbers are current-HEAD; re-`grep -n iterator_close_getter` before
each slice since earlier edits shift later line numbers.)

Each test file follows `skipped-elements-error-created-before-close.js`'s
structure:
- an iterator object with `__proto__: Iterator.prototype`, a `get next()`
  that throws `Test262Error` (proves the validation error fires before any
  iteration), and a `return()` that (a) reassigns the global error
  constructor under test to a fake constructor, (b) forces a collection via
  guarded `$262.gc()` (exercises the `gc_root_value`/`gc_unroot_value`
  bracket, not just the ordering), (c) returns `{}`, and (d) is observed to
  have run (a `closed` flag) so the test also confirms `IteratorClose` still
  happens;
- restore the original constructor in a `finally`;
- assert the thrown value's prototype is the *original* constructor's
  `.prototype`, not the fake's, and assert `closed === true`.

Use a plain object receiver (`__proto__: Iterator.prototype`, no other
Iterator-helper machinery) so the pre-existing missing-receiver-check gap on
`forEach`/`some`/`every`/`find`/`reduce` (see §7) never enters these tests.

## 6. Test surface

- New tests run via the directory-argument path (no dedicated test262-extra
  runner, per project convention):
  `uv run python scripts/run-test262.py test262-extra/built-ins/Iterator/prototype/`
- Targeted regression check against existing test262 coverage for the same
  helpers:
  `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/forEach/`
  (repeat for `some`, `every`, `find`, `reduce`, `map`, `filter`, `take`,
  `drop`, `flatMap`) — test262 itself has no ordering test for this defect
  (per the issue), so these runs exist to catch a regression in the
  surrounding logic the reorder touches, not to newly pass anything.
- Full suite: `uv run python scripts/run-test262.py`.
- Lint as its own command: `./scripts/lint.sh`.
- `cargo test --bin jsse` (per fmt/clippy-hook memory: this crate is bin-only).

## 7. Regression risk

Low. The change is a pure reorder plus one new private helper that wraps two
already-audited primitives (`gc_root_value`/`gc_unroot_value`,
`iterator_close_getter`) in the exact bracket pattern already shipped and
covered for `includes` in PR #558. It does not touch:
- the tree-walker hot paths (`eval_expr`/`exec_statement`) — the sites are
  all inside native `define_method` closures;
- `property.rs`'s MOP;
- the `ObjectKind` match in `gc::trace_object_fields` — no new object kind or
  GC root shape, just an already-existing rooting primitive used at more call
  sites;
- the bytecode fast path — these are host-native builtins, not
  bytecode-compiled user functions;
- the Node-compat library harnesses — none of the pinned libraries are
  documented as exercising this precise ordering.

The only way `test262-pass.txt` could move is upward (a currently-failing
test that happened to depend on this ordering starts passing); nothing in
scope changes control flow for the non-error path, so no currently-passing
test can regress. This plan does not roll the baseline forward — that stays a
`main`-branch operation.

## 8. Out of scope

- The `chunks`/`windows` helpers already construct their errors before
  closing (verified at `iterators.rs:2512-2520` and `:2653-2661`, both use
  `iterator_close_with_completion(interp, this, Err(err.clone()))` *after*
  `err` is built) — not touched, not part of the 14 sites, no regression
  risk from leaving them as-is.
- `forEach`, `some`, `every`, `find`, and `reduce` are missing the spec's
  "If O is not an Object, throw a TypeError exception" receiver check (step
  2) that `map`, `filter`, `take`, `drop`, `flatMap`, `includes`, and `join`
  all have (e.g. `Iterator.prototype.forEach.call(null, fn)` skips straight
  to the callable check instead of throwing per spec step 2). This is a
  distinct, pre-existing defect, not part of issue #569 — worth its own
  follow-up issue, not bundled here.
- The `take`/`drop` third RangeError branch (`limit > 2**53 - 1`, not present
  in the pinned `spec.html` text but required by test262's
  `limit-rangeerror.js`) is preserved exactly as-is; this plan reorders it
  along with the other two branches but does not investigate or resolve the
  spec/test262 text mismatch.
- No refactor of the surrounding helper-object closures (`map`'s/`filter`'s/
  `flatMap`'s `next`/`return` native closures untouched beyond the single
  validation site each has before the closure is built).

## 9. Suggested squash subject

`fix(iterator): create helper validation errors before IteratorClose`
