# Plan: issue #796 — root the iterator and resolve function in Promise.all/allSettled/race/any

## 1. Problem restated

`promise_all`, `promise_all_settled`, `promise_race`, and `promise_any` in
`src/interpreter/builtins/promise.rs` each open a GC temp-root frame and root
the new promise capability (`cap.promise`/`cap.resolve`/`cap.reject`), but two
other values that live only in Rust locals for the remainder of the function
are never rooted: `promise_resolve` (the constructor's `resolve` method,
fetched once and invoked once per loop iteration) and `iterator` (the
iterator record returned by `get_iterator`, stepped once per loop iteration).
Both are reachable from JS only transiently — a custom iterable's
`[Symbol.iterator]()` or a subclass's overridden `resolve` can return an
object with no other JS-side reference once control returns to the Rust
loop. If user code invoked synchronously from inside the loop (a custom
`resolve`, a custom iterator's `next`, or a `then` handler) requests a
collection, the collector cannot see `promise_resolve` or `iterator` as live,
frees the underlying object(s), and the next `iterator_step`/`call_function`
on them produces spec-impossible failures ("Iterator does not have a next
method", "`#<Object>` is not a function") instead of continuing the
algorithm.

## 2. Spec basis

- ECMA-262 §27.2.4.1 `Promise.all` (esid `sec-promise.all`) / §27.2.4.1.1
  `GetPromiseResolve` / §27.2.4.1.2 `PerformPromiseAll`: `promiseResolve` is
  obtained once via `GetPromiseResolve` and the `PerformPromiseAll`
  repeat-loop re-reads `iteratorRecord` and re-invokes `promiseResolve` on
  every iteration.
- ECMA-262 §27.2.4.2 `Promise.allSettled` (esid `sec-promise.allsettled`) /
  `PerformPromiseAllSettled`: same shape.
- ECMA-262 §27.2.4.3 `Promise.any` (esid `sec-promise.any`) /
  `PerformPromiseAny`: same shape.
- ECMA-262 §27.2.4.5 `Promise.race` (esid `sec-promise.race`) /
  `PerformPromiseRace`: same shape (no accumulator slots, but the same
  `iteratorRecord` + `promiseResolve` re-use across iterations).

(Exact step letters are not quoted here — the `spec/` submodule is empty in
this workspace; the implementation stage should run
`git submodule update --init --depth 1 spec` and grep `spec.html` for
`sec-performpromiseall`/`sec-performpromiseallsettled`/`sec-performpromiseany`/
`sec-performpromiserace` before writing the tests' `info:` fields, mirroring
how the existing sibling tests cite steps.)

No JavaScript-observable syntax or semantics change: each algorithm must
already produce the results in §4 when nothing collects mid-loop.
`iteratorRecord` and `promiseResolve` are the same abstract-operation-level
values across every repeat-loop iteration by spec construction; jsse
represents them as two plain Rust locals, and an unrooted local is simply an
implementation bug in holding onto a value the algorithm is still obligated
to use later. The fix makes jsse's GC agree with what the spec already
requires to stay reachable; it adds no new behavior and changes no spec
step.

## 3. Files to touch

- `src/interpreter/builtins/promise.rs` — the only production file. Four
  call sites, matching the issue's line numbers (current `main`, subject to
  drift during implementation):
  - `promise_all` (~line 1298, 1311): root `promise_resolve` and `iterator`
    right after each is obtained.
  - `promise_all_settled` (~line 1442, 1452): same.
  - `promise_race` (~line 2009, 2019): same.
  - `promise_any` (~line 2084, 2094): same.
  In each case the fix is two lines inside the existing
  `gc_root_frame()`/`gc_unroot_frame()` span already wrapping the closure:
  ```rust
  let promise_resolve = match self.get_object_property(ctor_id, "resolve", constructor) { ... };
  self.gc_root_value(&promise_resolve);   // new
  ...
  let iterator = match self.get_iterator(iterable) { ... };
  self.gc_root_value(&iterator);          // new
  ```
  `gc_unroot_frame(gc_frame)` already runs unconditionally after the closure
  on every exit path (the tail return and every early `return` inside the
  `(|| { ... })()`), so no separate unroot call is needed — this matches how
  `cap.promise`/`cap.resolve`/`cap.reject` and `slots.root()` are already
  handled in the same functions.
- `test262-extra/` — four new regression tests (one per combinator), listed
  in §4/§5. No existing test262-extra file is modified.
- No `docs/adr/` entry: this is a bug fix inside an already-documented GC
  rooting discipline (`CLAUDE.md` "GC Root-Stack Discipline"), not a new
  architectural decision. No `CONTEXT.md` change: no new vocabulary.

## 4. TDD slices

Each slice is one combinator: add the failing regression test, confirm red,
apply the two-line fix to that function only, confirm green, confirm the
pre-existing stress-only-failing sibling test (named in the issue) now also
passes under `JSSE_GC_STRESS`. Order is simplest-structure first.

**Test design constraint (why a naive port of the issue's one-liner repro is
not enough):** in all four functions, `promise_resolve` is fetched exactly
once via `Get(constructor, "resolve")`, *before* the loop, and reused by
calling the Rust local on every iteration — it is never looked up again. If
a test defines `Sub.resolve` as a plain *data* property (as the issue's
own inline repro does), the function value also stays reachable through
`Sub.resolve` itself for the whole test (`Sub` is a rooted global lexical
binding), so that local being unrooted is never actually exercised — such a
test would go red-to-green on the `iterator` line alone and give false
confidence about the `promise_resolve` line. To exercise both locals for
real, each test must:
1. Define `Sub.resolve` as an **accessor** whose `get` returns a **fresh**
   closure each time (so the only reference to that closure, after the
   one-time `Get`, is the engine's `promise_resolve` Rust local) — the same
   trick `Promise-combinator-sync-setup-gc-rooting.js` already uses for a
   different window.
2. Have that closure return a thenable object
   (`{ then(onFulfilled, onRejected) { ...; onFulfilled(value); } }`) whose
   `then` method calls `$262.gc()` on the *first* element only, and calls
   `$262.gc()` **inside `then`, not inside the closure itself** — i.e. after
   `call_function(&promise_resolve, ...)` for that element has already
   returned and `promise_resolve`/`iterator` are off the Rust call stack, so
   the test doesn't depend on whether a value is incidentally rooted for the
   duration of being on the stack as a callee/argument.
3. Use a plain array literal (`[1, 2, 3]`) as the iterable, so the default
   Array iterator object is never referenced from JS once `get_iterator`
   returns it — it is reachable only through the engine's `iterator` local.
4. Require at least 2 more loop iterations after the first (3-element input)
   so the test fails distinctly depending on which root is missing: without
   the `iterator` root, the second `iterator_step` throws "Iterator does not
   have a next method"; without the `promise_resolve` root, the second call
   to the (by-then-freed) resolve closure throws "... is not a function".
   The implementation stage should confirm both failure modes individually
   (temporarily keep only one of the two new `gc_root_value` lines) before
   relying on the test as proof that *both* lines are needed — a test that
   merely goes green is not proof it checked the thing it claims to.

1. **`promise_race`** (no accumulator slots — smallest diff).
   - Test: `test262-extra/Promise-race-resolve-iterator-reentry-gc-rooting.js`,
     built per the constraint above. `Promise.race.call(Sub, [1, 2, 3])`
     resolves with the first-settled value; assert the result is `1`. Red
     today: the second element's `iterator_step`/`promise_resolve` call (run
     after the first element's `then()` has already collected) fails.
   - Production: add the two `gc_root_value` calls in `promise_race`
     (`src/interpreter/builtins/promise.rs`, obtaining `promise_resolve` and
     `iterator`).
   - Green: the new test passes; re-run
     `Promise-race-sync-setup-gc-rooting.js` under `JSSE_GC_STRESS=1` and
     confirm it now passes too.
2. **`promise_any`** (same fulfillment-wins shape as `race`, but routes
   through `on_rejected`/`cap.resolve` directly rather than a slots-writing
   `on_fulfilled` — next-simplest).
   - Test: `test262-extra/Promise-any-resolve-iterator-reentry-gc-rooting.js`,
     same construction; `Promise.any.call(Sub, [1, 2, 3])` resolves with `1`.
   - Production: add the two `gc_root_value` calls in `promise_any`.
   - Green: new test passes; re-run `Promise-any-sync-setup-gc-rooting.js`
     under `JSSE_GC_STRESS=1` and confirm it now passes.
3. **`promise_all`** (adds the slots-writing `on_fulfilled` per-element
   closure to the mix — all 3 elements must process for the test to reach
   its assertion, not just the first 2).
   - Test: `test262-extra/Promise-all-resolve-iterator-reentry-gc-rooting.js`;
     only the first element's `then()` collects, elements 2 and 3 proceed
     normally. Assert `assert.compareArray(values, [1, 2, 3])` on the
     resolved array (requires all 3 loop iterations — hence all 3
     `promise_resolve`/`iterator_step` calls — to succeed).
   - Production: add the two `gc_root_value` calls in `promise_all`.
   - Green: new test passes; re-run
     `Promise-all-sync-setup-nextvalue-gc-rooting.js` under
     `JSSE_GC_STRESS=1` and confirm it now passes.
4. **`promise_all_settled`** (same shape as `promise_all`, different
   per-element record).
   - Test: `test262-extra/Promise-allSettled-resolve-iterator-reentry-gc-rooting.js`.
     Assert the resolved array is
     `[{status:"fulfilled",value:1}, {status:"fulfilled",value:2}, {status:"fulfilled",value:3}]`.
   - Production: add the two `gc_root_value` calls in `promise_all_settled`.
   - Green: new test passes; re-run `Promise-allSettled-sync-setup-gc-rooting.js`
     under `JSSE_GC_STRESS=1` and confirm it now passes.

Each slice is independently revertable and touches exactly one function plus
one new test file, satisfying "small, auditable" — do not land all four in
one commit.

## 5. Test surface

- New coverage (test262 has no `host-gc-required`/`$262.gc()` concept, so
  this class of test lives in `test262-extra/`, matching the five sibling
  files the issue names as already present there):
  - `test262-extra/Promise-race-resolve-iterator-reentry-gc-rooting.js`
  - `test262-extra/Promise-all-resolve-iterator-reentry-gc-rooting.js`
  - `test262-extra/Promise-allSettled-resolve-iterator-reentry-gc-rooting.js`
  - `test262-extra/Promise-any-resolve-iterator-reentry-gc-rooting.js`
  - Run with: `uv run python scripts/run-test262.py test262-extra/Promise-race-resolve-iterator-reentry-gc-rooting.js`
    (and similarly per file), then the whole directory:
    `uv run python scripts/run-test262.py test262-extra/`.
- Stress re-verification of the issue's named pre-existing tests (these
  already exist and already pass without stress; the issue's claim is that
  they fail *under* `JSSE_GC_STRESS`):
  `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/Promise-all-sync-setup-nextvalue-gc-rooting.js test262-extra/Promise-allSettled-sync-setup-gc-rooting.js test262-extra/Promise-any-sync-setup-gc-rooting.js test262-extra/Promise-combinator-sync-setup-gc-rooting.js test262-extra/Promise-race-sync-setup-gc-rooting.js`
  — the issue lists all five as currently failing under stress; re-run each
  by name before and after the four production fixes and record which ones
  flip to green. Do not assume in advance which fix each one depends on —
  `Promise-combinator-sync-setup-gc-rooting.js` in particular exercises a
  getter-returned, GC'd-during-fetch resolve closure over a plain array
  iterator, which overlaps this issue's two locals; whether it needs this
  fix or was already passing for an unrelated reason is a question for the
  actual stress run, not for this plan. If any of the five is still red
  after all four fixes land, that's a sixth call site or a different gap —
  treat it as a new finding, not a reason to special-case the test.
- Broad conformance baseline (no behavior change expected, run to catch
  regressions): `uv run python scripts/run-test262.py test262/test/built-ins/Promise/all/ test262/test/built-ins/Promise/allSettled/ test262/test/built-ins/Promise/race/ test262/test/built-ins/Promise/any/`.
  Requires `git submodule update --init --depth 1 test262` first if the
  submodule is empty in this workspace.
- `cargo build --profile release-checked` then re-run the same
  `test262-extra/` targets on that binary (`--binary target/release-checked/jsse`)
  to exercise the `debug_assert!` root-stack-balance checks the new
  `gc_root_value` calls must satisfy (per `CLAUDE.md` "GC Root-Stack
  Discipline").
- `cargo test --release` for the Rust unit/integration suite (unaffected by
  this change but part of the standard gate).
- `./scripts/lint.sh` before considering any slice done.

## 6. Regression risk

- Low risk of moving `test262-pass.txt`: the fix only adds GC roots: it
  changes *when memory is freed*, never a return value, thrown error, or
  control-flow path when no collection races with the loop. The four
  functions' non-GC behavior (iteration order, `then` wiring, settle
  values) is untouched.
- Leans on: `gc.rs` temp-root stack (`gc_root_value`/`gc_root_frame`/
  `gc_unroot_frame`) and the root-stack-balance `debug_assert!`s described in
  `CLAUDE.md`. Getting the two new `gc_root_value` calls placed *inside* the
  existing `gc_root_frame()`/`gc_unroot_frame()` span (not outside it) is
  the only way to keep that balance — placing them outside would either
  leak a root past the frame's bulk-unroot or double-pop.
- `JSSE_GC_STRESS` runs are the actual oracle for this class of bug;
  non-stress test262/test262-extra runs will not catch a regression here
  (that's exactly why the five pre-existing sibling tests silently had this
  gap). The new deterministic tests close that blind spot by forcing the
  collection unconditionally rather than relying on stress sampling.
- No interaction with the bytecode fast path (`bytecode/`): these
  combinators are native-function builtins invoked the same way regardless
  of which path compiled the caller.
- No interaction with the Node-compat library harnesses (none of
  `decimal.js`/`big.js`/`acorn`/etc. call `$262.gc()`; this bug is only
  observable under the test262 `$262` host hooks or `JSSE_GC_STRESS`), so no
  library-harness re-run is needed.

## 7. Out of scope

- `promise_all_keyed` and `promise_all_settled_keyed` (`Promise.allKeyed` /
  `Promise.allSettledKeyed`, the await-dictionary Stage-3 proposal,
  `src/interpreter/builtins/promise.rs` ~line 1616 and ~1773) have the same
  unrooted `promise_resolve` local — confirmed by reading both functions
  while investigating this issue. They are not named in #796 (its line
  numbers point only at `promise_all`/`promise_all_settled`/`promise_race`/
  `promise_any`), they aren't governed by any ECMA-262 clause (Stage-3,
  not-yet-standard), and they use `proxy_own_keys` rather than
  `get_iterator`, so there's no `iterator` local to root there — only
  `promise_resolve`. This is the same bug class but a distinct fix with its
  own line numbers; file a follow-up issue rather than bundling it into this
  PR ("many small changes beat one large change"). `test262-extra/Promise-allKeyed-sync-setup-gc-rooting.js`
  and `test262-extra/Promise-allSettledKeyed-sync-setup-gc-rooting.js` are
  the likely stress-affected siblings for that follow-up to check, by the
  same reasoning as §5 — verify empirically, don't assume.
- No refactor of the `promise_all`/`promise_all_settled`/`promise_race`/
  `promise_any` functions beyond the two-line addition per function — in
  particular, no extraction of a shared "root resolve+iterator" helper even
  though the four call sites are now textually identical. A follow-up could
  propose that as a deepening opportunity, but it's not part of closing this
  bug.
- The per-element native closures `on_fulfilled` (in `promise_all` /
  `promise_all_settled`) and `on_rejected` (in `promise_any`) are created via
  `self.create_function(...)` and then only *pinned onto* by
  `self.pin_native_root(&on_fulfilled, &cap.resolve)` /
  `slots.pin_on(self, &on_fulfilled)` — those calls pin other things onto
  the closure as an anchor, they do not root the closure itself. Between
  creation and being passed to `call_function(&then_fn, &p, &[on_fulfilled,
  reject_fn])`, the closure sits in a Rust local the same way
  `promise_resolve`/`iterator` did; a `then` *getter* on the element promise
  that requests a collection in that window could free it before it's ever
  installed. This is the same bug class as #796 but a different local, not
  named in the issue, and rooting it per iteration interacts with the
  strictly-LIFO temp-root stack (`CLAUDE.md` "GC Root-Stack Discipline") in
  a way that deserves its own design rather than being folded into this
  fix. File it as a follow-up; `Promise-any-combinator-gc-rooting.js` and
  the sibling `all`/`allSettled` combinator tests cover the *already-fixed*
  pinning behavior, not this gap, so they won't catch it either way.
- No `--update-baseline` run; `test262-pass.txt` is not touched by this
  branch.
