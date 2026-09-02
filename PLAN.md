# Plan: issue #571 — Iterator helpers lose the cached `next` method to GC

## 1. Problem restated

`GetIteratorDirect` reads an iterator's `next` property once and the resulting Iterator
Record (`[[Iterator]]`, `[[NextMethod]]`) is used for every step of a helper method's
`Repeat` loop. In jsse, the six eager consuming methods on `Iterator.prototype`
(`toArray`, `forEach`, `some`, `every`, `find`, `reduce`) plus `join` hold that Iterator
Record only as native Rust locals (`iter`, `next_method`) for the duration of the loop —
nothing in the GC-traced object graph roots them. When `next` is an accessor that returns
a freshly allocated function (so no other heap object owns it), a major collection
triggered from user code reached during iteration (a `value` getter, `next()` itself, or
ordinary allocation pressure) can reclaim `next_method` mid-loop. The following iteration
step then calls a dead object and throws `TypeError: ... is not a function`. PR #558 fixed
this for `Iterator.prototype.includes` by scope-rooting the Iterator Record (and each
step's result object) across the loop; this issue is "do the same for the rest."

## 2. Spec basis

- `sec-getiteratordirect` (`GetIteratorDirect`, spec/spec.html:6987) — reads `next` via
  `Get` exactly once and returns the Iterator Record `{ [[Iterator]], [[NextMethod]],
  [[Done]] }`. The record's `[[NextMethod]]` is a single captured value, not re-read on
  each step — the abstract operation itself requires the *same* function object to answer
  every step of the loop that follows.
- `sec-iteratorstepvalue` (`IteratorStepValue`, spec/spec.html:7141), which composes
  `sec-iteratorstep` (spec/spec.html:7117) → `sec-iteratornext` (spec/spec.html:7066,
  calls `Call(_iteratorRecord_.[[NextMethod]], ...)`) and `sec-iteratorvalue`
  (spec/spec.html:7104, `Get(_iteratorResult_, "value")`). Each helper's `Repeat` loop
  calls this once per step against the *same* Iterator Record produced by step 1's
  `GetIteratorDirect`.
- Per-method algorithms, all of the shape "`Set iterated to ? GetIteratorDirect(O)`, then
  `Repeat: Let value be ? IteratorStepValue(iterated)`":
  `sec-iterator.prototype.every` (spec/spec.html:48421),
  `sec-iterator.prototype.find` (spec/spec.html:48471),
  `sec-iterator.prototype.foreach` (spec/spec.html:48532),
  `sec-iterator.prototype.reduce` (spec/spec.html:48580),
  `sec-iterator.prototype.some` (spec/spec.html:48608),
  `sec-iterator.prototype.toarray` (spec/spec.html:48664).
- `Iterator.prototype.join` is not present in this pinned `spec/spec.html` snapshot
  (submodule ref `270a490`), but test262 already ships tests for it under esid
  `sec-iterator.prototype.join` (e.g.
  `test262/test/built-ins/Iterator/prototype/join/separator-placement-empty-values.js`),
  and jsse's own implementation (`src/interpreter/builtins/iterators.rs:1829`) follows the
  identical `GetIteratorDirect` + per-step `next`-call shape as the six methods above, so
  it is governed by the same abstract operations and is in scope for the identical fix.
- This is purely a heap-management/GC-rooting correctness bug in the implementation, not a
  change to any observable JavaScript syntax or semantics: the fix makes jsse conform to
  the Iterator Record's implicit "same `[[NextMethod]]` for the whole loop" invariant that
  the spec already requires; it does not alter what any conforming program observes.

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs` — production fix, seven call sites:
  - `toArray` (line ~1445)
  - `forEach` (line ~1472)
  - `some` (line ~1514)
  - `every` (line ~1566)
  - `find` (line ~1617)
  - `reduce` (line ~1763)
  - `join` (line ~1829), plus the shared helper `iterator_step_value_getter` (line ~482)
    that `join` (and `chunks`/`windows`/`take`/`drop`/`flatMap`) call to read a step's
    result — see slice 8 below.
- `test262-extra/built-ins/Iterator/prototype/{toArray,forEach,some,every,find,reduce,join}/next-method-rooted-across-iteration.js`
  — seven new tests, one per method, following the pattern already established at
  `test262-extra/built-ins/Iterator/prototype/includes/next-method-rooted-across-iteration.js`.
- No `docs/adr/` entry: this follows an already-established, documented pattern (the
  scope-rooting idiom from #558, and the `set_helper_gc_roots`/`gc_native_roots` idiom
  already used by `map`/`filter`/`take`/`drop`/`flatMap`), not a new architectural
  decision. No `CONTEXT.md` vocabulary changes.

## 4. TDD slices

Line numbers below are as of this plan's writing; each slice's edits shift the lines
below it, so locate call sites by `define_method(iter_proto_id, "<name>"` rather than by
line number once implementation starts.

Each slice: add the red test262-extra test (must fail on the current `main` + this
branch, reproducing the reported `TypeError: ... is not a function` under a release
build), then make it pass with the minimal production change, mirroring `includes`
(`src/interpreter/builtins/iterators.rs:1719-1759`) exactly: wrap the consuming loop with
`let frame = interp.gc_root_frame(); interp.gc_root_value(&iter);
interp.gc_root_value(&next_method); ... interp.gc_unroot_frame(frame);`, convert every
`return` inside the loop to `break <Completion>` so there is a single exit point, and root
each step's `result` object across the `interp.iterator_value(&result)` call (`result` is
itself unrooted while its own `value` getter runs and can be collected the same way).

1. **`toArray`** — red: `test262-extra/.../toArray/next-method-rooted-across-iteration.js`
   drives `makeIterator().toArray()` with a `next` accessor returning a fresh closure and
   a `value` getter calling `$262.gc()`; asserts the resulting array is `[1,2,3,4,5,6]`.
   Green: wrap the `loop` at `iterators.rs:1451-1466` per the pattern above.
2. **`forEach`** — red: same generator shape, asserts the callback observed values
   `1..6` in order via a `compareArray` check. Green: wrap `iterators.rs:1489-1509`.
3. **`some`** — red: asserts `true` when the target value is produced under GC pressure,
   `false` when the iterator exhausts under GC pressure. Green: wrap
   `iterators.rs:1531-1562`.
4. **`every`** — red: asserts `true` when every produced value passes under GC pressure,
   `false` on the first failing value. Green: wrap `iterators.rs:1583-1613`.
5. **`find`** — red: same three assertions as the existing `includes` test (found,
   not-found, found-after-skip is not applicable here, so: found mid-iteration,
   not-found after full exhaustion). Green: wrap `iterators.rs:1634-1664`.
6. **`reduce`** — red: covers both the explicit-initial-value form and the
   no-initial-value form (which itself calls `iterator_step_direct` once *before* the main
   loop to seed the accumulator — that pre-loop step must be inside the same rooted frame,
   since it uses the same `iter`/`next_method` locals). Green: start the frame
   immediately after `get_iterator_direct_getter` succeeds (`iterators.rs:1775`), so it
   covers both the seed step (`iterators.rs:1785-1799`) and the main loop
   (`iterators.rs:1801-1824`).
7. **`join`** — red: same shape, asserts the joined string is correct under GC pressure
   (e.g. `"1,2,3,4,5,6"`). Green: wrap `iterators.rs:1849-1886`'s loop the same way. This
   alone is not sufficient — see slice 8.
8. **Harden `iterator_step_value_getter`'s own `result` handling** — this shared helper
   (`iterators.rs:482-511`) calls `next_method`, then reads `.done` and `.value` off the
   returned `result` object via property getters, while holding `result` only as a Rust
   local. Root `result` immediately after the `call_function` at line 487, then either
   `gc_unroot_frame` before each of the five subsequent `return`s (lines 489, 490, 493,
   498, 502, 507) or restructure the function to a single exit point.
   Unverified dependency — check empirically before assuming slice 7 needs this: during
   the `.value` getter call, `result` is the getter's `this` binding, which the active
   call's environment chain may already keep reachable, in which case slice 7's `join`
   test could go green without this change. If so, this slice has no red test of its own;
   land it as defensive hardening (matching what `includes` already does for the identical
   reason, per that fix's own comment) guarded by the existing test262 `join`/`chunks`/
   `windows`/`take`/`drop`/`flatMap` suites rather than a new failing test. One case where
   `result` genuinely is at risk regardless: a Proxy result whose `get` trap triggers GC —
   the trap runs with the handler as its own binding, not rooted via `result`'s call
   frame — so the hardening is not vacuous even if the plain-accessor case turns out to
   already pass. This is the same shared primitive used by `chunks`, `windows`, `take`,
   `drop`, and `flatMap`'s lazy-helper `next` closures, so this fix also closes their
   identical latent (unreported) exposure — re-run their existing test262 directories as
   regression coverage. No new slices are added for those lazy helpers: their `iter`/
   `next_method` rooting is already handled via `set_helper_gc_roots`/`gc_native_roots` and
   they have no reported reproducer.

## 5. Test surface

- Targeted test262 runs (must stay green — no spec-mandated behavior changes):
  `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/toArray/`
  `.../forEach/` `.../some/` `.../every/` `.../find/` `.../reduce/` `.../join/`.
- Regression coverage for the shared-helper touch in slice 8:
  `uv run python scripts/run-test262.py test262/test/built-ins/Iterator/prototype/chunks/`
  `.../windows/` `.../take/` `.../drop/` `.../flatMap/`.
- test262 has no coverage for the GC-rooting scenario itself (confirmed by the issue and
  by inspection of the above directories), so the seven new
  `test262-extra/built-ins/Iterator/prototype/<method>/next-method-rooted-across-iteration.js`
  files are the actual regression tests for this bug. Run them with:
  `uv run python scripts/run-test262.py test262-extra/built-ins/Iterator/prototype/`.
- `cargo test --bin jsse` (per repo convention: crate is bin-only, so `--bin jsse` rather
  than the default `cargo test`) to catch anything in the Rust-level test suite that
  exercises these builtins.
- Full `uv run python scripts/run-test262.py` regression run before considering the change
  complete, comparing against the `origin/main:test262-pass.txt` baseline (no
  `--update-baseline`, per repo convention — that's a `main`-branch operation).

## 6. Regression risk

- **GC rooting stack discipline**: `gc_root_frame`/`gc_unroot_frame` is index-based
  (truncates `gc_temp_roots` back to a saved length), so it tolerates any number of
  `gc_root_value` calls pushed after the frame marker without needing matched pop calls —
  this is why converting every loop-internal `return` to `break` first is required: a
  `return` that bypasses the trailing `gc_unroot_frame(frame)` would leak roots onto
  `gc_temp_roots` for the rest of the call (not a correctness bug per se, since the next
  top-level `gc_unroot_frame`/safepoint boundary would eventually clean it up incorrectly
  by truncating too much or too little) — the single-exit shape is what makes the frame
  discipline provably correct, exactly as call out in the issue's own fix-shape note.
- **Shared helper touch (slice 8)**: `iterator_step_value_getter` has 10 call sites across
  `chunks`, `windows`, `join`, the `take`/`drop` lazy-helper `next` closures, and the
  `flatMap` inner-iterator loop. A mistake there risks regressing all of them, not just
  `join` — mitigated by running their full existing test262 directories, not just the new
  `join` test.
- **GC safepoint / mark-and-sweep**: all seven fixes lean on `gc.rs`'s existing
  `gc_temp_roots` root set, already walked by `trace_object_fields`/`collect_value_roots`
  and exercised by `includes` and `iterate_to_vec` — no new GC machinery, so risk is
  confined to correct use of the existing API, not to `gc.rs` itself.
- **Tree-walker vs bytecode**: these are native Rust builtin functions invoked identically
  regardless of whether the calling JS runs through the tree-walker or the (default-off)
  bytecode fast path, so no interaction expected with `bytecode_enabled`/`dispatch_body`.
- **`test262-pass.txt` baseline**: the fix should not change pass/fail status for any
  existing test262 test (it only prevents a use-after-free-style bug from firing under GC
  pressure that ordinary test262 runs don't trigger), so the full regression run is a
  safety check, not an expected-diff review.

## 7. Out of scope

- The RAII iterator-record rooting guard proposed in issue #331 (root-on-construct,
  unroot-on-drop) — would unify all these call sites under one construct and eliminate the
  whole bug class at once, but is a materially larger refactor than this bug-fix PR should
  carry. Left as a follow-up; worth referencing from the PR description so it isn't lost.
- Adding explicit per-call-site `result` rooting to `chunks`, `windows`, `take`, `drop`,
  and `flatMap`'s own call sites of `iterator_step_value_getter` — already covered
  structurally by the slice-8 fix inside the shared helper; no separate touch needed.
- Any change to `map`/`filter`/`take`/`drop`/`flatMap`'s `iter`/`next_method` rooting —
  already correct via `set_helper_gc_roots`/`gc_native_roots` (verified by reading its 10
  call sites at `iterators.rs:2020,2160,2321,2493,2635,2784,3073,3426,3729,4045`, and the
  generic trace of `obj.gc_native_roots` at `gc.rs:859`), and not part of the issue's
  reported defect.
- The related error-rooting issue #569 — a different rooting gap, not bundled here.
- Rewording or "fixing" any existing test262 test — none of the affected directories'
  existing tests are wrong; they simply don't cover this scenario.
- No `test262-pass.txt` baseline update — that is a `main`-branch operation per repo
  convention and this branch does not target rolling it forward.
