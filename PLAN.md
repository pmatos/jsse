# Plan: issue #820 — gc: Object.fromEntries leaves obj_id/step/key/value unrooted under JSSE_GC_STRESS

## 1. Problem restated

`Object.fromEntries`'s native implementation
(`src/interpreter/builtins/mod.rs:6519-6587`) creates the result object
(`obj_id`/`obj_val`) and calls `get_iterator` *before* the function's single
`with_gc_root_scope` block starts, then inside the loop extracts the
per-iteration iterator-result (`step`), the entry receiver (`next_item`),
the raw key (`key_raw`), and the entry value (`value`) into plain Rust
locals without rooting any of them. Every one of these values is reachable
only through a Rust stack variable in the native closure, not through a GC
root — the closure never executes as JS, so nothing binds them into an
environment the collector walks. `AddEntriesFromIterable` (used by
`fromEntries`) calls back into arbitrary user code at several points —
`GetIterator`'s own `[Symbol.iterator]()` call, the iterator's `next()`, the
`"0"`/`"1"` accessor getters on the entry object, and `ToPropertyKey`'s
`@@toPrimitive`/`toString`/`valueOf` calls on a non-primitive key — and each
such call can execute JS statements, which is exactly where
`JSSE_GC_STRESS` forces a collection. When that collection lands, `obj_id`
(always — it is never rooted at all in the current code) or a per-iteration
local is unreachable from `collect_gc_roots`, gets swept, and its arena slot
may be recycled — producing wrong-typed reads or (as observed) a
`to_primitive` call on a stale/recycled `key_raw` object falling through
every branch to `TypeError: Cannot convert object to primitive value`.

Confirmed on this workspace: `cargo build --profile release-checked` then
`JSSE_GC_STRESS=1 uv run python scripts/run-test262.py --binary
target/release-checked/jsse test262/test/built-ins/Object/fromEntries/ -j 8`
reproduces exactly the 4 regressions from the issue (`evaluation-order.js`,
`evaluation-order.js:strict`, `to-property-key.js`,
`to-property-key.js:strict`); the unstressed run is 50/50.
`evaluation-order.js`'s entries use JS-authored `get '0'`/`get '1'`
accessors, a `toString` method on the key, and a JS-authored
`[Symbol.iterator]`, so it fails deterministically under
`JSSE_GC_STRESS=1` with no `$262.gc()` needed — read in full it is the
clearest live demonstration of every one of the four unrooted locals
(`obj_id` is live across the whole thing; `step`/`next_item`/`key_raw` each
get exposed to a nested user-code call that doesn't involve them as
receiver).

The async-generator `yield*` cluster described in the same issue body is
already tracked and actively worked as its own issue, **#828**
("`fix(gc): async-generator yield* error constructor identity swaps under GC
stress`"), separate from this one. #820's own title and the exact repro in
its body point specifically at `Object.fromEntries`; this plan scopes #820
to that cluster only — see "Out of scope" below.

## 2. Spec basis

- `sec-object.fromentries` — `Object.fromEntries ( iterable )`: creates
  `obj` via `OrdinaryObjectCreate`, builds an internal `adder` closure over
  `obj` that does `? ToPropertyKey(key)` then
  `! CreateDataPropertyOrThrow(obj, propertyKey, value)`, then returns
  `? AddEntriesFromIterable(obj, iterable, adder)`.
- `sec-add-entries-from-iterable` — `AddEntriesFromIterable ( target,
  iterable, adder )`: `? GetIterator(iterable, sync)`, then repeat
  `? IteratorStepValue(iteratorRecord)` (drives the iterator's `next()`),
  check the step's value is an Object, `Get(next, "0")`,
  `IfAbruptCloseIterator`, `Get(next, "1")`, `IfAbruptCloseIterator`,
  `Call(adder, target, « k, v »)` (which runs `ToPropertyKey` and the
  define), `IfAbruptCloseIterator`.
- `sec-topropertykey` — `ToPropertyKey ( argument )`: for a non-primitive
  `argument`, calls `? ToPrimitive(argument, string)`, which per
  `sec-toprimitive` can invoke a user `@@toPrimitive` method or, failing
  that, `OrdinaryToPrimitive`'s `toString`/`valueOf` — all user-code call
  sites.

This is a pure GC-rooting engine bug: no spec clause's observable semantics
change. The fix makes the engine's existing (already-spec-correct, per the
passing non-stressed baseline) control flow safe against a collection
landing mid-algorithm; it does not alter what `AddEntriesFromIterable`
computes.

## 3. Files to touch

- `src/interpreter/builtins/mod.rs` — the `Object.fromEntries` native
  closure, `fromEntries` entry around lines 6519-6587. No other builtin in
  this file is in scope (`Object.groupBy`/`Map.groupBy` already have their
  own fix landed in #837; `Map`'s constructor, which shares
  `AddEntriesFromIterable` conceptually but is a separate native closure, is
  not touched — see "Out of scope").
- `test262-extra/Object-fromEntries-under-construction-gc-rooting.js` — new
  regression test (shape below).
- No `src/interpreter/gc.rs`, `docs/adr/`, or `CONTEXT.md` changes: the fix
  reuses the existing `with_gc_root_scope`/`gc_root_value`/`gc_unroot_value`
  primitives, the same ones the `Object.groupBy` (#837) and
  `Iterator.prototype.toArray`/`forEach` (`src/interpreter/builtins/iterators.rs:1604-1660`)
  fixes already use for an equivalent "root a per-iteration transient,
  release it once its last use has passed" pattern. No new rooting
  primitive or vocabulary is introduced, and nothing here reopens #806
  (closed/released) — fromEntries already uses `with_gc_root_scope` at the
  top level; this fix widens what it covers and layers precise
  per-iteration `gc_root_value`/`gc_unroot_value` pairs inside it, matching
  `iterators.rs`'s existing style for the same shape of loop.

## 4. TDD slices

1. **Red — add the regression test against today's code:** add
   `test262-extra/Object-fromEntries-under-construction-gc-rooting.js`
   (shape in section 5) and confirm it **fails on an unmodified plain
   release build** — `cargo build --release` then `uv run python
   scripts/run-test262.py test262-extra/Object-fromEntries-under-construction-gc-rooting.js`
   — with no `JSSE_GC_STRESS` needed, since the test forces collection
   itself via `$262.gc()`. This is the step that proves the test actually
   exercises the bug rather than passing vacuously; if it passes
   unmodified, the test isn't reaching the gap and needs reshaping (e.g. the
   `$262.gc()` call isn't placed where a still-needed local is unrooted) before
   moving on.

2. **Confirm the existing test262 regression too:** `cargo build --profile
   release-checked` then `JSSE_GC_STRESS=1 uv run python
   scripts/run-test262.py --binary target/release-checked/jsse
   test262/test/built-ins/Object/fromEntries/ -j 8` shows the 4 failures
   recorded in section 1 against the 50/50 unstressed baseline. No code
   change in this step either — it's the second red confirmation, using the
   stress knob instead of the new test's explicit `$262.gc()`.

3. **Green, root the result object across `GetIterator` too:** widen the
   `with_gc_root_scope` closure to start *before* `create_object_id()` and
   *before* `get_iterator` is called (both currently happen outside it).
   Inside the scope: create `obj_id`/`obj_val`, root it immediately
   (`interp.gc_root_value(&obj_val)`), *then* call `get_iterator` and root
   `iterator` the same way the current code already does. `obj_val` must be
   rooted before `get_iterator` runs because `evaluation-order.js`'s
   `[Symbol.iterator]()` is itself user code that can trigger a stress
   collection before any entry is processed.

4. **Green, root the per-iteration locals in strict push/pop order:**
   inside the loop, in this exact order (matching the `RootStack`'s strict
   LIFO discipline — `gc_unroot_id`/`pop_expected` debug-asserts the id
   being released is the top entry):
   - root `step` right after `iterator_step` returns it; unroot it
     immediately after `iterator_value(&step)` returns `next_item` — nothing
     else has been pushed yet, so this pop is already top-of-stack.
   - root `next_item` right after that (before the `"0"` fetch).
   - root `key_raw` right after the `"0"` fetch (before the `"1"` fetch and
     before `to_property_key`).
   - root `value` right after the `"1"` fetch (before `to_property_key`,
     which can run user code via `key_raw`'s `ToPrimitive` and must not
     leave `value` exposed).
   - after `insert_value(key, value)` succeeds, unroot in exact reverse push
     order: `value`, then `key_raw`, then `next_item`.

   The throw paths (`iterator_close` + `return Completion::Throw(...)`)
   deliberately do **not** unroot their way back out by hand first — the
   outer `with_gc_root_scope` truncates everything back to its saved frame
   on every exit path of the closure, including these early returns
   (`src/interpreter/mod.rs:1471-1491` documents this guarantee), so an
   early throw with `next_item`/`key_raw` still rooted is correct, not a
   leak.

5. **Green, full targeted re-run:** re-run both commands from slices 1 and
   2 — expect the new test262-extra test to pass and the targeted test262
   directory to go back to 50/50 with zero regressions.

6. **Full suite:** `cargo build --release` then `uv run python
   scripts/run-test262.py` to confirm no regression against
   `origin/main:test262-pass.txt`, plus `cargo test` (debug) to exercise the
   `debug_assert!` root-stack-balance checks across the restructured
   push/pop pairs.

## 5. Test surface

- New deterministic regression test:
  `test262-extra/Object-fromEntries-under-construction-gc-rooting.js`. Model
  it on `test262-extra/Object-groupBy-under-construction-gc-rooting.js`'s
  `collect()` helper (`$262.gc()` plus array churn to make the freed slot's
  reuse likely) and on
  `test262/test/built-ins/Object/fromEntries/evaluation-order.js`'s
  accessor-based entries, but call `collect()` explicitly instead of
  relying on `JSSE_GC_STRESS`:
  - `[Symbol.iterator]` calls `collect()` before returning the iterator —
    exercises `obj_val` staying rooted across `GetIterator`.
  - each entry's `get '0'` getter returns a **fresh** object (not held by
    any other JS variable) whose `toString` calls `collect()` before
    returning the key string — exercises `key_raw`/`next_item` staying
    rooted across `ToPropertyKey`, since nothing else in the test keeps
    that key object alive.
  - each entry's `get '1'` getter calls `collect()` before returning a
    **fresh** object as the value — exercises `value` staying rooted across
    the following `ToPropertyKey` call on the *next* entry's key, and
    `next_item` staying rooted across the `"1"` fetch itself.
  - assert the final object's own keys match the expected labels, and that
    each value is (or carries) the distinct object identity returned by
    that entry's `"1"` getter (e.g. a property set on it), so a
    silently-dropped or wrong-identity value fails the assertion instead of
    just happening to still look right.
  - `features: [host-gc-required]`, same as the groupBy test.
  - Per slice 1, this test must **fail** on the current (pre-fix) binary
    with a plain `cargo build --release` — no `JSSE_GC_STRESS` needed — and
    pass after the fix in slice 3-4.
- Targeted test262, with and without stress, to confirm the existing suite
  tests pass: `uv run python scripts/run-test262.py
  test262/test/built-ins/Object/fromEntries/` (plain) and
  `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py --binary
  target/release-checked/jsse test262/test/built-ins/Object/fromEntries/ -j
  8` (stressed, `release-checked` binary).
- Full test262 run before opening the PR, per AGENTS.md: `cargo build
  --release` then `uv run python scripts/run-test262.py` — confirms no
  unrelated regression against `origin/main:test262-pass.txt`.
- The new `test262-extra/` test is what actually gates the fix in CI:
  `ci.yml`'s blocking `JSSE_GC_STRESS=7` pass only covers `test262-extra/`,
  not the full `test262/` suite (the full-suite stress pass is the
  non-blocking nightly sample), so relying on
  `evaluation-order.js`/`to-property-key.js` alone would not give
  deterministic CI coverage of this fix.
- `cargo test` (debug) to pick up the `debug_assert!` root-stack-balance
  checks in `gc_assert_root_depth`/`RootStack` across the new root/unroot
  pairs.

## 6. Regression risk

- The change is confined to one native closure; it cannot move bytecode vs.
  tree-walker dispatch, so no risk to the bytecode fast path or
  `ObjectKind` matches.
- Risk is almost entirely to the `gc_temp_roots` `RootStack` LIFO discipline
  documented in AGENTS.md: the new `gc_root_value`/`gc_unroot_value` calls
  must be released in exact reverse-of-push order (`step`; then `value`,
  `key_raw`, `next_item` together at iteration end) — see slice 4's ordering
  rationale. A misordered pop trips `pop_expected`'s `debug_assert!`
  immediately under `cargo test` or the `release-checked` profile (which
  `ci.yml` already runs over `test262-extra/`), so this is self-checking,
  not just reasoned-about.
- Could move `test262-pass.txt` only in the unlikely case the restructured
  closure changes observable behavior (e.g. a typo that skips an entry or
  double-inserts one) — slice 5's full targeted re-run and slice 6's
  full-suite run are the guard for that.
- No interaction with the property MOP (`property.rs`) beyond the existing
  `get_object_property`/`to_property_key` calls already in the closure —
  nothing there changes shape, only when roots are pushed/popped around the
  existing calls.
- No interaction with the Node-compat library harnesses; `Object.fromEntries`
  is exercised by several of them (e.g. `zod`, `moment`) but only through
  its normal (non-stressed) behavior, which this fix does not alter.

## 7. Out of scope

- The async-generator `yield*` stress-fragility cluster from the issue body
  (14 regressions under `async-gen-method`/`async-gen-private-method`
  `yield-star-*`) — already tracked and in progress as **#828**. Not
  touched here; no new issue needed.
- `Map`'s constructor / `Map.groupBy`'s own `AddEntriesFromIterable`-shaped
  native closures: `Map`'s constructor has a structurally similar entry loop
  but is a separate closure with its own potential gap. Fixing it is not
  needed to close #820 (which is scoped to `Object.fromEntries` by title and
  the reproduced symptom) and bundling it in would widen this PR's diff
  beyond one closure. If triage later finds the same gap there, it should be
  its own small PR, not folded into this one.
- No refactor of the broader iterator-protocol helpers
  (`iterator_step`/`iterator_value`/`to_property_key`/`to_primitive`) to make
  receiver-rooting automatic/structural. That's a legitimate deepening
  opportunity (it would remove the need for every call site to reason about
  this by hand) but is a larger, higher-blast-radius change than a bug fix
  PR should carry; left as a candidate for `.architecture/backlog.md` rather
  than bundled here.
- No baseline update (`--update-baseline` is a `main`-branch operation and is
  not part of this PR).
