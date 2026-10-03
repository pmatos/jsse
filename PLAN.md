# Plan: issue #825 — async-dispose throw-return-getter loses rejection under GC stress

## 1. Problem restated

`%AsyncIteratorPrototype%[Symbol.asyncDispose]` is implemented as a single native
closure (`src/interpreter/builtins/iterators.rs`, `setup_async_generator_prototype`,
the `async_dispose_fn` bound around line 4224) that creates a `PromiseCapability`
(`cap`) up front and then threads `cap.promise` / `cap.resolve` / `cap.reject`,
plus several further intermediate `JsValue`s (`return_method`, `result`,
`result_wrapper`, `on_fulfilled`, and any caught error `e`), through calls that
can run arbitrary user JavaScript (`interp.obj_get(this, "return")` evaluates a
user-defined `return` getter; `interp.call_function(&return_method, this, &[])`
invokes a user-defined `return()` method). None of these locals are pushed onto
the GC temp-root stack. They are ordinary Rust-stack `JsValue`s, invisible to
`collect_gc_roots`, so if a safepoint fires while the getter/method body runs
and nothing else in the object graph references the capability's promise, the
collector reclaims it and a later allocation can recycle its arena id.

Confirmed empirically with two minimal repros (both run against this branch's
release build, no `JSSE_GC_STRESS` needed — a direct `$262.gc()` call inside
the user callback is enough):
- `return` getter throws, calling `$262.gc()` first: the returned value's
  `typeof` is `"object"` but `.then` is `undefined` — `cap.promise`'s arena id
  was recycled into some other plain object.
- `return()` method resolves normally, calling `$262.gc()` first: the returned
  value's `typeof` is `"function"` — recycled into something else entirely.

Both are the same missing-root bug, reached via the two different user-code
callback points in the same closure. This is the same class of bug already
fixed at the sibling `DisposableStack.prototype.dispose`/
`AsyncDisposableStack.prototype.disposeAsync` call site
(`src/interpreter/builtins/disposable.rs`) and at `Promise.all`
(`src/interpreter/builtins/promise.rs`, `promise_all`) via `with_gc_root_scope`
+ `gc_root_value`; `AsyncIteratorPrototype[Symbol.asyncDispose]` was never
brought in line with that convention.

## 2. Spec basis

- `%AsyncIteratorPrototype%[Symbol.asyncDispose]` is defined by the
  **explicit-resource-management** proposal, not by the ecma262 mainline text
  pinned in this repo's `spec/` submodule. Verified directly: `grep -ni
  "asyncdispose" spec/spec.html` returns nothing; `grep -ni
  "asynciteratorprototype" spec/spec.html` finds only
  `sec-%asynciteratorprototype%-%symbol.asynciterator%` (the `@@asyncIterator`
  method) and the two `[[Prototype]]` cross-references to it — no
  `@@asyncDispose` clause exists at this pinned commit. The proposal has not
  landed in this `spec/` checkout.
- Per this project's stated authority order — "(1) ECMAScript spec, (2)
  test262, (3) node" — when the spec genuinely does not reach a feature, the
  next authority is test262 itself. The test262 test this issue is about
  (`test262/test/built-ins/AsyncIteratorPrototype/Symbol.asyncDispose/throw-return-getter.js`)
  carries `esid: sec-%asynciteratorprototype%-@@asyncDispose` and an `info:`
  block that reproduces the proposal's algorithm verbatim (steps 1–4: `NewPromiseCapability`,
  `GetMethod(O, "return")`, `IfAbruptRejectPromise`). That `esid`/`info:` pair
  is the governing basis for this fix, not a `spec/` line number, because none
  exists yet.
- Critically, this plan does **not** propose any change to that algorithm or
  to any observable behavior it produces. `iterators.rs`'s `async_dispose_fn`
  already implements the steps correctly — the existing inline `// N.` comments
  in the closure map 1:1 to the test's `info:` block, and the test262 test
  already passes with no `JSSE_GC_STRESS` set. The bug is a violation of an
  implementation-level invariant with no spec vocabulary at all: GC root-stack
  discipline, documented in this project's `CLAUDE.md` under "GC Root-Stack
  Discipline" and `src/interpreter/root_stack.rs`. Fixing it makes the already
  spec-correct algorithm hold under all execution conditions instead of only
  some; it introduces no new syntax, no new semantics, and no new deviation
  from the algorithm's steps.

## 3. Files to touch

- `src/interpreter/builtins/iterators.rs` — the `async_dispose_fn` closure
  inside `setup_async_generator_prototype` (currently lines ~4224–4356). Add
  GC rooting; no other function in this file changes.
- `test262-extra/AsyncIteratorPrototype-asyncDispose-return-getter-throws-gc-rooting.js`
  — new regression test (primary; matches the issue's exact scenario: `return`
  getter throws).
- `test262-extra/AsyncIteratorPrototype-asyncDispose-return-method-gc-rooting.js`
  — new regression test (the `return()` method resolves normally; covers the
  `call_function(return_method)` → `promise_resolve_with_constructor` →
  `create_function(on_fulfilled)` → `PerformPromiseThen` leg of the same
  closure, which the primary test does not reach and which is independently
  confirmed broken today — see slice 1).

No `docs/adr/` entry: this follows an existing, already-documented convention
(`with_gc_root_scope` + `gc_root_value`, "GC Root-Stack Discipline" in
`CLAUDE.md`) rather than introducing a new one.

## 4. TDD slices

1. **Red (both scenarios, before any production change):**
   - Add `test262-extra/AsyncIteratorPrototype-asyncDispose-return-getter-throws-gc-rooting.js`,
     modeled on the existing `test262-extra/*-gc-rooting.js` house style (see
     `AsyncGenerator-next-request-promise-gc-rooting.js` for the pattern: no
     `includes`, a raw `.then(onOk, onErr)` chain ending in
     `.then($DONE, $DONE)`, `flags: [async]`,
     `features: [explicit-resource-management, host-gc-required]`). Scenario:
     an object whose `return` getter calls `$262.gc()` then throws a distinct
     error constructor; dispose via
     `AsyncIteratorPrototype[Symbol.asyncDispose].call(obj)`; assert
     `Object.getPrototypeOf(p) === Promise.prototype` *before* calling `.then`
     (so a regression fails with a clear assertion message instead of a bare
     "undefined is not a function" from calling `.then` on a recycled
     non-promise); then assert the promise rejects with that exact error.
   - Add `test262-extra/AsyncIteratorPrototype-asyncDispose-return-method-gc-rooting.js`:
     an object whose `return()` method calls `$262.gc()` and returns a plain
     (non-thenable) value; same `Object.getPrototypeOf(p) === Promise.prototype`
     guard; assert the disposal promise fulfills with `undefined` per steps
     6–7 of the algorithm.
   - Confirm both fail today:
     `uv run python scripts/run-test262.py test262-extra/AsyncIteratorPrototype-asyncDispose-return-getter-throws-gc-rooting.js`
     and the `-return-method-` sibling. (Manually confirmed during planning
     with throwaway scripts in `$TMPDIR`, outside the repo: the getter-throws
     case returns an object whose `.then` is `undefined`; the return-method
     case returns a `typeof "function"` value. Both are recycled arena ids,
     not the capability's actual promise. The committed test files are new —
     they have not been run yet — so this step is restating what the
     throwaway scripts showed, as the first real run of the real files.)
2. **Green:** in `iterators.rs`'s `async_dispose_fn`, wrap the body from
   immediately after `cap` is constructed through the final return in
   `interp.with_gc_root_scope(|interp| { ... })` (mirroring
   `disposable.rs`'s `async_disposable_stack_dispose`/`dispose` and
   `promise.rs`'s `promise_all`, both read directly during planning). Immediately
   after entering the scope, root `cap.promise`, `cap.resolve`, and
   `cap.reject` with `gc_root_value`. Root each further `JsValue` local the
   moment it is produced and before the next call that can run user code or
   allocate: `return_method` (before the `Call(return, O, «»)` step), `result`
   (before `promise_resolve_with_constructor`), `result_wrapper` (before
   `create_function` for `on_fulfilled`), `on_fulfilled` (before it is moved
   into the `PromiseReaction`s), and the caught `e` in both `Err` branches
   (before `reject_promise`, matching the defensive style already used for
   the caught error in `disposable.rs`'s dispose loop). Re-run both tests from
   slice 1 — both must now pass.
3. **Refactor:** none planned. The closure already follows the step-by-step
   spec-comment style used elsewhere in this file; adding rooting calls does
   not warrant restructuring it further. (If the diff ends up hard to read
   inline, extracting the post-`cap` body into a named helper taking `&cap` is
   an acceptable minor refactor *within this same change*, not a follow-up —
   but only if it falls out naturally from the `with_gc_root_scope` closure
   boundary already required for the fix.)

## 5. Test surface

- Targeted test262 run (must stay green, confirms no regression on the
  existing spec-conformance tests for this method):
  `uv run python scripts/run-test262.py test262/test/built-ins/AsyncIteratorPrototype/`
- Targeted test262 run under GC stress, matching the issue's original
  discovery path (should now pass where it previously failed):
  `JSSE_GC_STRESS=2 uv run python scripts/run-test262.py test262/test/built-ins/AsyncIteratorPrototype/ --timeout 300`
- New regression coverage (spec-correct behavior under a deterministic
  `$262.gc()` call that test262 itself cannot express, since test262 has no
  concept of a host GC stress mode):
  `uv run python scripts/run-test262.py test262-extra/AsyncIteratorPrototype-asyncDispose-return-getter-throws-gc-rooting.js`
  and the `-return-method-` sibling.
- The same two new files under the exact CI gates that block merges on this
  project, both of which this change must pass before it's considered done:
  - `JSSE_GC_STRESS=7` sweep over `test262-extra/`, normal and `--bytecode`:
    `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/`
    and the same command with `--bytecode` added to the adapter (per
    `ci.yml`'s existing job shape).
  - `release-checked` build over `test262-extra/`, normal and `--bytecode`
    (catches root-stack push/pop imbalance via `gc_assert_root_depth`):
    `cargo build --profile release-checked` then
    `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/`
    and again with `--bytecode`.
- Full custom-test and test262-extra sweep before declaring done:
  `uv run python scripts/run-custom-tests.py`
  `uv run python scripts/run-test262.py test262-extra/`
- Full test262 run to confirm the baseline doesn't regress (do **not** pass
  `--update-baseline`; this is a feature-branch run only):
  `uv run python scripts/run-test262.py`
- `cargo test --release` — covers `src/interpreter/dispose.rs` and
  `src/interpreter/tests.rs` unit/integration tests that already exercise
  `Symbol.asyncDispose`/`AsyncDisposableStack` adjacent paths, as a smoke
  check that the rooting change doesn't disturb normal (non-stress) control
  flow.

## 6. Regression risk

- **Scope is narrow:** the change is additive (root-stack pushes/pops) inside
  one native closure; it does not alter control flow, return values, or
  timing of promise settlement on any path, so it should not move
  `test262-pass.txt` in either direction under normal (non-stress) execution.
  The only way it could move the baseline is if a `gc_root_value` call were
  placed incorrectly (e.g., rooting a value whose id is `0`/sentinel and
  masking a different bug) — mitigated by following the exact, already-read
  pattern from `promise_all` (`builtins/promise.rs`) and `disposable.rs`'s
  dispose loop verbatim, rather than inventing a new rooting shape.
- **Shared machinery leaned on:** `gc_temp_roots` / `RootStack`
  (`src/interpreter/root_stack.rs`, `with_gc_root_scope` in
  `src/interpreter/mod.rs`) and `gc_safepoint()` (`src/interpreter/gc.rs`) —
  this is exactly the machinery the fix uses, not machinery it changes.
  `gc_assert_root_depth` runs after every native call in debug/
  `release-checked` builds, so any push/pop mismatch introduced here (e.g., a
  scope exited without unrooting, or a double-unroot) will fail loudly under
  `cargo build --profile release-checked` rather than silently — this is
  listed explicitly in §5 as a required gate, not just a risk note.
- **Not touched, so not at risk:** the tree-walker's `eval_expr`/`exec_statement`
  hot paths, `property.rs`'s MOP dispatch, the exhaustive `ObjectKind` match,
  the bytecode VM/compiler, and the Node-compat library harnesses — none of
  these are reachable from this closure in a way this change alters.
- **Interacts with:** `new_promise_capability`'s fast path
  (`builtins/promise.rs`) and `create_resolving_functions` (same file) — both
  read during planning to confirm `cap.resolve`/`cap.reject`'s native
  closures capture only a raw `promise_id: u64` and `Rc<Cell<bool>>`, never a
  `JsValue`, so no `pin_native_root` is needed in addition to the transient
  `gc_root_value` calls this fix adds — only temporary root-stack entries for
  the duration of this one native call.

## 7. Out of scope

- Auditing or fixing other native closures for the same missing-root pattern
  beyond `AsyncIteratorPrototype[Symbol.asyncDispose]` itself. If the
  implementation stage notices a sibling bug while here (e.g., in
  `IteratorPrototype[Symbol.dispose]` or elsewhere), it is logged as a new
  issue, not folded into this PR.
- Any refactor of `PromiseCapability`/`new_promise_capability` to root its own
  result automatically (e.g., having `new_promise_capability` return an
  already-rooted guard type). That would touch every one of the dozen call
  sites in `builtins/promise.rs` and is a larger, separate architectural
  change — not a bug fix.
- Formatting or comment cleanup elsewhere in `iterators.rs` unrelated to the
  closure being fixed.
- Extending `$262.gc()`-based regression coverage to other
  `explicit-resource-management` builtins (`DisposableStack`,
  `AsyncDisposableStack`, `using`/`await using` desugaring) — those are not
  reported broken by this issue and already have their own GC-stress coverage
  path (`disposable.rs` already uses `with_gc_root_scope`).
