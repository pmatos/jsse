# Plan: issue #827 — fix(gc): Map.groupBy loses iterator's next method under GC stress

## 1. Problem restated

Issue #827 reports that the nightly `JSSE_GC_STRESS` run observed `TypeError: Iterator
does not have a next method` from `Map.groupBy`, pointing at an unrooted iterator
record/result object surviving a collection mid-iteration. Investigation (see the
`gh issue comment` posted on #827) found that this exact hazard in `Map.groupBy`
(`src/interpreter/builtins/collections.rs:628-766`) was already fixed, same day, as a
side effect of `35a490e3` (#832, merged 2026-10-03T18:40:58Z, ~3h42m after this issue
was filed): that commit wraps the closure body in `with_gc_root_scope` and roots both
`iterator` (line 658) and `result_val` (line 676), even though PR #832's own
description claimed `Map.groupBy` was out of its scope. Reproduction attempts against
current `main` (hand-built harness+test concatenation and the real
`scripts/run-test262.py`, swept across `JSSE_GC_STRESS=1..50`, strict/sloppy,
tree-walker/`--bytecode`, with `--features perf-counters` confirming collections are
genuinely firing) all pass — the named bug is gone.

The identical hazard is **not** gone: `Object.groupBy`
(`src/interpreter/builtins/mod.rs:5725-5818`) implements the same spec `GroupBy`
abstract operation and has the same shape (`GetIterator`, then a loop that creates a
result object and calls a user callback before returning it), but was never wrapped in
`with_gc_root_scope`. It reproduces the issue's exact symptom deterministically with
test262's own `test262/test/built-ins/Object/groupBy/evenOdd.js`, at
`JSSE_GC_STRESS=1/2/3`, tree-walker and `--bytecode`. This plan fixes `Object.groupBy`
using the same idiom already applied to `Map.groupBy`, and adds the
`test262-extra/` regression coverage the issue explicitly asked for ("Needs a minimal
repro plus `$262.gc()` ... then a `test262-extra/` regression once fixed") — none
exists today for either `groupBy` entry point.

## 2. Spec basis

- `GroupBy ( items, callback, keyCoercion )` — `spec/spec.html`, clause
  `sec-groupby`. Step 4: `Let iteratorRecord be ? GetIterator(items, sync)`. Then
  `Repeat`: `Let next be ? IteratorStepValue(iteratorRecord)`;
  `Let key be Completion(Call(callback, undefined, « value, 𝔽(k) »))` with
  `IfAbruptCloseIterator`; `Perform AddValueToKeyedGroup(groups, key, value)`
  (clause `sec-add-value-to-keyed-group`). The Iterator Record and the `groups` list
  must remain the *same* object across every step of `Repeat`, each of which (the
  callback call, in particular) can run arbitrary user code.
- `Object.groupBy ( items, callback )` — clause `sec-object.groupby`. Step 1:
  `Let groups be ? GroupBy(items, callback, ~property~)`. This is the function with
  the live bug.
- `Map.groupBy ( items, callback )` — clause `sec-map.groupby`. Step 1:
  `Let groups be ? GroupBy(items, callback, ~collection~)`. Already correctly rooted;
  used here only as the reference shape for the fix, and as a second regression-test
  target per the issue's own ask.
- `GetIterator ( obj, kind )` / `GetIteratorFromMethod` / `GetIteratorDirect` — reached
  by `interp.get_iterator()`. Not modified by this plan (see §7); cited only because
  it's the shared helper both `groupBy` implementations call.

This plan changes no JS-observable syntax or semantics. `Object.groupBy`'s
spec-mandated behavior is unchanged; the fix only prevents an internal GC-stress
memory-safety bug (a freed/recycled arena id read back as a wrong-typed object) from
producing a non-spec-compliant `TypeError` partway through an otherwise-conforming
`Repeat` loop. The behavior being restored — the Iterator Record and `groups` staying
the same object throughout — is exactly what `GroupBy`'s abstract-operation steps
require by construction.

## 3. Files to touch

- `src/interpreter/builtins/mod.rs` — `Object.groupBy`'s native closure
  (~lines 5725-5818, inside `setup_object_statics`/global `Object` setup). Wrap the
  body from just after `let iterator = match interp.get_iterator(&items) { ... };`
  through the final `Completion::Normal(result_val)` in
  `interp.with_gc_root_scope(|interp| { ... })`, rooting `iterator` immediately and
  `result_val` right after it's created — mirroring
  `src/interpreter/builtins/collections.rs:657-676` (`Map.groupBy`'s already-landed
  fix) exactly in shape.
- `test262-extra/Object-groupBy-under-construction-gc-rooting.js` (new) — regression
  test, red before the fix / green after, modeled on the existing
  `test262-extra/Map-constructor-under-construction-gc-rooting.js` (same
  `$262.gc()`-in-callback idiom, same `features: [host-gc-required]` tag, same
  `esid`-pointing-at-the-real-clause convention).
- `test262-extra/Map-groupBy-under-construction-gc-rooting.js` (new) — same-shape
  regression test for `Map.groupBy`. This one is green both before and after the
  `mod.rs` change in this PR (it depends only on the already-landed #832 fix); it
  exists purely to close the "needs a test262-extra/ regression once fixed" gap this
  issue asked for, since #832's own new tests covered only the four named
  constructors (`Map`/`Set`/`WeakMap`/`WeakSet`), not `Map.groupBy`.

No `docs/adr/` entry: this applies an existing, already-documented idiom
(`with_gc_root_scope`/`gc_root_value`, described in `CLAUDE.md`'s "GC Root-Stack
Discipline" section) to a second call site. It is not a new architectural decision.

## 4. TDD slices

1. **Red** — add `test262-extra/Object-groupBy-under-construction-gc-rooting.js`:
   call `Object.groupBy([1, 2, 3], cb)` where `cb` calls `$262.gc()` on every
   invocation before returning `'even'`/`'odd'` by parity, then assert the result
   object's shape (`Object.keys(result)` via `assert.compareArray`, and each group's
   array contents). Run it with `uv run python scripts/run-test262.py
   test262-extra/Object-groupBy-under-construction-gc-rooting.js` against the
   current (unfixed) binary; confirm it fails with `TypeError: Iterator does not have
   a next method` (or an equivalent wrong-typed-object symptom).
2. **Green** — apply the `with_gc_root_scope`/`gc_root_value` wrap described in §3 to
   `Object.groupBy` in `src/interpreter/builtins/mod.rs`. Rebuild; re-run slice 1's
   test; confirm it passes.
3. **Lock in the sibling fix** — add
   `test262-extra/Map-groupBy-under-construction-gc-rooting.js`, the same-shape test
   against `Map.groupBy`. No production code changes in this slice; confirm it's
   already green (documents and guards the #832 side-effect fix against regression).
4. **Gate** — run the full test surface in §5 to confirm no baseline movement and no
   new failures anywhere else.

## 5. Test surface

- Targeted test262, plain and `--bytecode`:
  `uv run python scripts/run-test262.py test262/test/built-ins/Object/groupBy/
  test262/test/built-ins/Map/groupBy/` — expect 100% pass, 0 regressions against the
  `origin/main` baseline (unchanged by this fix; GC-rooting-only).
- The two new regression tests, individually and under stress (matching the project's
  own CI gate): `uv run python scripts/run-test262.py
  test262-extra/Object-groupBy-under-construction-gc-rooting.js
  test262-extra/Map-groupBy-under-construction-gc-rooting.js`, then the same with
  `JSSE_GC_STRESS=1` and `JSSE_GC_STRESS=7`.
- Full `test262-extra/` (plain, `--bytecode`, and `JSSE_GC_STRESS=7`), matching
  `ci.yml`'s blocking gate, with no new failures.
- Full `uv run python scripts/run-test262.py` (no path filter) to confirm the
  `Object.groupBy` change moves nothing else in `test262/` (expect 0 regressions, 0
  new passes — identical in kind to #832's own test plan for its four constructor
  fixes).
- `cargo build --profile release-checked` then `test262-extra/` (normal and
  `--bytecode`) to confirm the root-stack LIFO/balance debug-asserts hold for the new
  `with_gc_root_scope` usage.
- `cargo test --release` (Rust unit suite) and `uv run python
  scripts/run-custom-tests.py` (`tests/` suite) as the standard gate; neither is
  expected to change.
- `./scripts/lint.sh`.
- No `scripts/`, `.github/`, or Node-compat library-harness surface is touched, so
  `run-library-tests.sh`/shim fixtures are not part of this change's gate.

## 6. Regression risk

- The change only adds root-stack pushes/pops (`gc_root_value`/`with_gc_root_scope`,
  `src/interpreter/mod.rs:1434-1491`) around `Object.groupBy`'s existing control flow.
  It does not change property-access order, error messages, or return values on any
  non-GC-stress path, so it carries effectively zero risk to `test262-pass.txt` under
  plain execution — confirm empirically with the full-suite run in §5 (expect 0
  regressions, 0 new passes, as #832 saw for its own four sites).
- Leans on: `with_gc_root_scope`/`gc_root_value`/`gc_root_frame`/`gc_unroot_frame`
  (`src/interpreter/mod.rs:1434-1491`) — the same machinery already exercised by
  `Map.groupBy`, the four collection constructors (#832), and
  `Array.prototype.concat`/`slice`. No changes to `gc.rs`, the exhaustive `ObjectKind`
  matches, the bytecode VM, or the tree-walker's `eval_expr`/`exec_statement` hot
  paths.
- The two new `test262-extra/` files are additive only; they cannot regress any
  existing `test262-extra` test, and `Map-groupBy-under-construction-gc-rooting.js`
  touches no production code at all.
- Watch for: the `release-checked` + `test262-extra/` debug-assert run (§5) is the
  thing that would actually catch a mis-balanced `with_gc_root_scope` (e.g. an early
  `return Completion::Throw(...)` inside the new closure that isn't covered by the
  scope's unconditional truncate-on-every-exit — `with_gc_root_scope` already handles
  this correctly by construction, but the review should re-check every `return` added
  inside the new closure boundary).

## 7. Out of scope

- **Fixing `get_iterator`'s and `iterator_value`'s own internal unrooted windows**
  (`src/interpreter/builtins/iterators.rs:4547-4556`, `:4900-4910`): the freshly
  created iterator/result object is a bare Rust local while fetching a `"next"`/
  `"value"` property that could be a user-defined getter or Proxy trap. This is a
  systemic gap shared by every `get_iterator` caller, including the now-fixed
  `Map.groupBy` and the now-fixed-by-this-plan `Object.groupBy` — only reachable with
  a hostile getter/Proxy-based iterable, not the plain-array repros this issue and
  this plan use. Flagged in the `gh issue comment` on #827 as a separate follow-up.
- **The `IfAbruptCloseIterator` gap** in both `groupBy` implementations: a `Throw`
  from the callback returns directly instead of first calling `IteratorClose` on the
  iterator record. A correctness gap, not a GC-safety bug, and unrelated to this
  issue's symptom — not bundled here.
- **Replacing `iterator_next_cache`** (the side-table `get_iterator` uses to stash the
  cached `next` method) **with the `RootedPair` abstraction** already used by
  `%IteratorPrototype%` helpers (`src/interpreter/builtins/iterators.rs:351-386`) —
  an architecture-level deepening, not a bug fix; belongs in the
  `.architecture/backlog.md` improve-codebase-architecture track.
- **No shared-helper refactor** of `AddValueToKeyedGroup`'s hand-inlined duplication
  between `Map.groupBy` and `Object.groupBy` — a pure refactor with no test-coverage
  change; this PR's job is the minimal fix plus the regression tests the issue asked
  for.
- **No `test262-pass.txt` baseline update** — not available from a feature branch.
