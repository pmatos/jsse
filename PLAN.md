# Plan: issue #645 — async disposal (`await using` / `AsyncDisposableStack.prototype.disposeAsync`) settles at the wrong microtask tick

## 1. Problem restated

The issue reports "one tick early" for `hint=async-dispose` resources whose dispose method is `undefined`
(`await using a = null`, `stack.use(null)`). Reproducing on `target/release/jsse` (HEAD `7e698cd`) and
comparing shape-by-shape against Node shows the report is a symptom of a broader, single root cause, and that
two details in the issue text are wrong (§1.2).

### 1.1 Root cause

Every await inside disposal goes through `await_value` (`src/interpreter/eval.rs:9568`), which is a
**blocking** await: it enqueues the continuation, then *drains the microtask queue inline* until that
continuation has run, and returns the value. `Await` in the spec **suspends** the running execution context
and returns control to the caller; the continuation is a later job. Consequences:

* Disposal never suspends the user's async function or the `disposeAsync()` caller. Queued microtasks
  (`w1`, `w2`, …) run *in the middle of synchronous code* — `Promise.resolve().then(m1); f(); log('sync')`
  logs `m1` before `sync` when `f` contains `await using`. That is the real defect.
* `dispose_resources` (`src/interpreter/exec.rs:2666`, used by `await using`) awaits once **per async
  resource** and never awaits for `null`/`undefined` resources. `async_disposable_stack_dispose`
  (`src/interpreter/builtins/disposable.rs:913`) is a second, structurally different copy that awaits per
  async resource and does a `needs_await`/`has_awaited` final await. Neither implements DisposeResources step
  3.d (see §2), and they disagree with each other on counts.
* The async-function executor papers over one shape with `pending_async_dispose_await`
  (`exec.rs:1062-1070`, `eval.rs:8506-8526`, `generator_transform.rs:641-656`): after a *block* containing
  `await using` it adds one extra real suspension on top of the blocking drain. That gets some block shapes
  right by accident and over-fires on others (row B3: one tick **late**).
* `disposeAsync()` (`disposable.rs:513-525`) runs the whole disposal synchronously, then returns an
  *already-resolved* promise via `create_resolved_promise`. Its `.then` callbacks are therefore enqueued at
  call time instead of after the disposal's Await.

### 1.2 Corrections to the issue text (implementation stage must not build from the issue's wording)

1. The issue says the spec requires "a real `Await(undefined)` per such resource — exactly one microtask
   tick per null/undefined resource". **False.** DisposeResources step 4 is a *single trailing*
   `Await(undefined)` gated on `needsAwait && !hasAwaited`. Node confirms: 1, 2 and 3 null resources all settle
   at the same tick (rows A1/A2). Do **not** implement a per-resource await.
2. "Both paths are off by the same one tick" is only true for the issue's own repro. The matrix below shows
   three distinct defects: (a) function-level `await using` and *every* non-empty `disposeAsync()` are early;
   (b) a block `await using` after a prior `await` is **late** (B3); (c) blocking drains run other jobs before
   synchronous code continues (all rows: `w1` before `sync-end`).
3. The issue cites "ECMA-262 27.3.3.2". The pinned `spec/` submodule (tc39/ecma262 `270a490`, Jan 2026)
   predates the Explicit Resource Management merge (`grep -ci asyncdispose spec/spec.html` → 0). See §2.

### 1.3 Evidence matrix (release binary, HEAD `7e698cd`; Node v26 as a *reference* matching the spec text)

Harness (`$TMPDIR/p/shapes.js`): a witness chain `Promise.resolve().then(w1).then(w2).then(w3).then(w4)` is
started, then the shape runs, then `settled` is logged from `p.then(...)`, then `sync-end` is logged
synchronously. `body`/`disposer`/`sync`/`after-block` are logged by the shape itself.

| # | shape | Node (expected) | jsse today |
|---|-------|-----------------|------------|
| A1 | `ADS.use(null)`; `disposeAsync()` | `sync-end,w1,w2,settled,w3,w4` | `w1,sync-end,w2,settled,w3,w4` |
| A2 | 3 × `use(null)` | same as A1 (count-invariant) | `w1,sync-end,w2,settled,…` |
| A3 | empty stack | `sync-end,w1,settled,w2,w3,w4` | identical (already correct) |
| A4 | `defer(async …)` | `disposer,sync-end,w1,w2,settled,w3,w4` | `disposer,w1,sync-end,w2,settled,…` |
| A5 | two async `defer`s | `d2,sync-end,w1,d1,w2,w3,settled,w4` | `d2,w1,d1,w2,sync-end,w3,settled,w4` |
| A6 | sync `defer` | `sync,sync-end,w1,w2,settled,w3,w4` | `sync,w1,sync-end,w2,settled,…` |
| A7 | async disposer rejects | `sync-end,w1,w2,rej1,w3,settled,w4` | `w1,sync-end,w2,rej1,w3,settled,w4` |
| B1 | fn-level `await using a = null` | `body,sync-end,w1,w2,settled,w3,w4` | `body,w1,sync-end,w2,settled,…` |
| B2 | block `await using null` (no prior await) | `body,sync-end,w1,after-block,w2,settled,w3,w4` | `body,w1,sync-end,w2,after-block,w3,settled,w4` |
| B3 | `await 0;` then block `await using null` | `sync-end,w1,body,w2,after-block,w3,settled,w4` | `sync-end,w1,body,w2,w3,after-block,w4,settled` (late) |
| B4 | fn-level async disposer | `body,disposer,sync-end,w1,w2,settled,w3,w4` | `body,disposer,w1,sync-end,w2,settled,…` |
| B5 | fn-level, then `throw 1` | `sync-end,w1,w2,rejected,w3,w4` | `w1,sync-end,w2,rejected,…` |
| B6 | fn-level, then `return 7` | `sync-end,w1,w2,settled,w3,w4` | `w1,sync-end,w2,settled,…` |
| B7 | `using s` (sync) then `await using n = null` | `body,sync-end,w1,sync-disposed,w2,settled,w3,w4` | `body,w1,sync-disposed,sync-end,w2,settled,…` |
| B8 | two `await using` nulls, fn-level | `body,sync-end,w1,w2,settled,w3,w4` | `body,w1,w2,sync-end,w3,settled,w4` (per-resource drain) |

`settled` lands at the right *relative* position in several jsse rows only because the early drain cancels the
early resolution; the invariant that is violated everywhere is `sync-end` before `w1`.

Passing test262 tests that touch this area — `AsyncDisposableStack/prototype/disposeAsync/explicit-await-for-null.js`,
`…-for-undefined.js`, `…-skipped-when-empty.js`, `language/statements/await-using/await-using-implies-await-if-evaluated.js`,
`…-does-not-imply-await-if-not-evaluated.js` — **already pass with the bug present** (verified by running them
against the built binary). They are not regression guards for tick exactness; the new `test262-extra/` tests
below pin it.

## 2. Spec basis

`spec/` (ecma262 `270a490`) does not contain Explicit Resource Management, so the governing text is the
proposal spec, `tc39/proposal-explicit-resource-management` `spec.emu` (cloned at plan time; main
`38c1329`), whose DisposeResources algorithm is quoted verbatim in the test262 files' `info:` blocks
(e.g. `built-ins/AsyncDisposableStack/prototype/disposeAsync/explicit-await-for-null.js`). Clauses:

* **DisposeResources ( disposeCapability, completion )** (`sec-disposeresources`). Load-bearing steps:
  * step 1–2: `needsAwait = false`, `hasAwaited = false`.
  * step 3.d: `hint` is `sync-dispose` ∧ `needsAwait` ∧ ¬`hasAwaited` ⇒ `Await(undefined)`, `needsAwait = false`.
  * step 3.e: `method` present ⇒ `Call(method, value)`; if normal ∧ `async-dispose` ⇒ `Await(result)`, `hasAwaited = true`;
    a throw (from the call *or* the await) becomes/`SuppressedError`-wraps `completion`.
  * step 3.f: `method` undefined ⇒ `needsAwait = true` (the `null`/`undefined` case).
  * step 4: `needsAwait` ∧ ¬`hasAwaited` ⇒ a single `Await(undefined)`.
* **Dispose ( V, hint, method )** (`sec-dispose`) and **GetDisposeMethod** (`sec-getdisposemethod`).
* **AsyncDisposableStack.prototype.disposeAsync ( )** (`sec-asyncdisposablestack.prototype.disposeAsync`):
  creates a promise capability *first*, runs DisposeResources, then resolves/rejects that capability
  (`IfAbruptRejectPromise`). Its promise is therefore settled only after DisposeResources' Awaits.
* **Await ( value )** (`await`, ecma262 §27.7.5.3 — present in `spec/spec.html:51047`): `PromiseResolve` +
  `PerformPromiseThen`, *suspend*, continue in a later job. **PromiseResolve** (`sec-promise-resolve`,
  `spec.html:49690`) and **PerformPromiseThen** (`sec-performpromisethen`, `spec.html:49825`).
* **AsyncBlockStart** (`sec-asyncblockstart`, `spec.html:51009`): async function completion resolves/rejects
  the result promise; the body's DisposeResources (via the function-body `await using` handling) precedes it.
* `await using` block/for/function scopes: the proposal's evaluation changes that call DisposeResources at
  scope exit (block, `for`, `for-of`, `switch`, function body, module).

test262 (authority 2) corroborates: `explicit-await-for-null.js`, `explicit-await-skipped-when-empty.js`,
`await-using-implies-await-if-evaluated.js` (esids `sec-asyncdisposablestack.prototype.disposeAsync`,
`sec-let-and-const-declarations-runtime-semantics-evaluation`). Node agrees with the spec text on every row of
§1.3 and is used only as a cross-check.

## 3. Files to touch

Slice A (ADS, self-contained, independently shippable):
* `src/interpreter/dispose.rs` (**new**, registered in `src/interpreter/mod.rs`) — a pure, resumable
  DisposeResources cursor (§4 slice 2). No tree-walker or MOP dependency.
* `src/interpreter/builtins/disposable.rs` — `disposeAsync` builtin (`:513`) and
  `async_disposable_stack_dispose` (`:913`) rewritten to drive the cursor with promise reactions.
* `src/interpreter/mod.rs` — module registration; (slice B) remove `pending_async_dispose_await` (`:299`, `:639`).

Slice B (`await using` in async functions):
* `src/interpreter/eval.rs` — `async_function_resume` (`~8180`+): `route_return!` (`:8399`), uncaught-throw
  route (`:8648`), `async_fn_complete` (`:9388`), post-body completion handling, and the flag check at
  `:8506`; a suspension helper alongside `async_fn_suspend_at_await` (`:9412`).
* `src/interpreter/exec.rs` — `Statement::Block` arm (`:1060-1072`); `dispose_resources` (`:2666`) becomes a
  thin blocking driver over the cursor (keeps current behavior for every non-suspendable caller).
* `src/interpreter/scheduler.rs` — `AsyncFunctionState` gains a `pending_dispose` field (GC-traced, see §6).
* `src/interpreter/generator_transform.rs` (`:641-656`) and `generator_analysis.rs` (`:858`) — only if the
  block-boundary state emitted for `await using` blocks needs adjusting; keep changes minimal.
* `src/interpreter/gc.rs` — trace the resources/values held by a suspended disposal.

Tests / docs:
* `test262-extra/async-disposable-stack-dispose-async-tick-alignment.js` (**new**),
  `test262-extra/await-using-fn-level-suspends-at-dispose.js`,
  `test262-extra/await-using-block-dispose-tick-alignment.js`,
  `test262-extra/await-using-dispose-resources-single-trailing-await.js`,
  `test262-extra/await-using-dispose-resources-sync-after-null-await.js` (**new**, names final at
  implementation time; each file's `esid`/`info` cites the DisposeResources step it pins).
* No `docs/adr/` entry expected; `CONTEXT.md` gets one line for the new vocabulary ("dispose cursor") only if
  the type is exported beyond `dispose.rs`.

## 4. TDD slices

Convention: every "red" step is a `test262-extra/` file (format of existing files: `/*--- esid, description,
info, flags: [async], includes: [asyncHelpers.js, compareArray.js], features: [explicit-resource-management]
---*/`, `asyncTest`) that fails on the current binary and encodes the §1.3 `Node (expected)` column. Run with
`uv run python scripts/run-test262.py test262-extra/<file>.js`. Assert with `compareArray` on the log so the
tick position of `settled` relative to the witness chain is what is pinned (the same technique as
`explicit-await-for-null.js`). Each slice is one commit.

**Slice A — `AsyncDisposableStack.prototype.disposeAsync` (rows A1–A7)**

1. **Red**: `async-disposable-stack-dispose-async-tick-alignment.js` with rows A1, A2 (count-invariance: 1 and 3
   nulls give the *same* sequence), A4, A5, A6, A7. Also assert row A3 (empty stack) so the fix cannot regress
   the one already-correct shape, and that `disposeAsync()` returns a **pending** promise for non-empty stacks
   (e.g. `sync-end` logged before any witness).
2. **Refactor (no behavior change) + green prep**: add `src/interpreter/dispose.rs` with
   `DisposeCursor { remaining: Vec<DisposableResource> /*reverse order*/, completion, needs_await, has_awaited }`
   and `fn step(&mut self, interp, awaited: Option<Result<JsValue,JsValue>>) -> DisposeStep`, where
   `DisposeStep::{Await(JsValue), Done(Completion)}`. It encodes DisposeResources literally, including
   step 3.d (sync-dispose after `needsAwait`), step 3.e (`SuppressedError` via the existing
   `wrap_suppressed_error`, a rejected Await counts as a throw completion for that resource) and step 4; it
   propagates `Completion::Exit` (issue #242) immediately without running further disposers, like today.
   Unit-test the cursor's step sequence in `src/interpreter/tests.rs` (pure inputs; assert the exact
   `Await` count for: 0/1/3 nulls, null+async, async+null, sync-after-null).
3. **Green**: `disposeAsync` per spec: create the promise capability (`create_promise_object` +
   `create_resolving_functions`), run the cursor; on `DisposeStep::Await(v)` do
   `promise_resolve_value(v)` + `perform_promise_then` with native fulfill/reject closures that re-enter the
   cursor (`perform_promise_then` at `promise.rs:1015`, `promise_resolve_value` at `:1191`; rooting per
   §6); on `Done` resolve/reject the capability. Keep the `Completion::Exit` propagation contract on the
   builtin's return (`disposable.rs:514-524`). `this` without `[[AsyncDisposableState]]` still returns a
   rejected promise; already-disposed still returns a resolved promise, with **no** Await.
   Run: the new file, `test262/test/built-ins/AsyncDisposableStack/`, `tests/` (`cargo test --release`).

**Slice B — `await using` in async functions (rows B1–B8)**

4. **Red**: `await-using-fn-level-suspends-at-dispose.js` (B1, B4, B5, B6), plus
   `…-single-trailing-await.js` (B8, and 1/2/3 nulls being count-invariant) and
   `…-sync-after-null-await.js` (B7: `Await(undefined)` happens *before* the sync disposer runs, so
   `sync-disposed` lands after `w1`). Add a regression for the microtask-mid-sync symptom itself:
   `Promise.resolve().then(m1); f(); log('sync')` must log `sync` before `m1`.
5. **Green (function-level exits)**: give the executor a *suspendable* driver. In `async_function_resume`,
   the three terminal dispose sites — `route_return!` (no enclosing finally), the uncaught-throw route, and
   `async_fn_complete` — call the cursor; on `Await(v)` they suspend exactly like `async_fn_suspend_at_await`
   (same reaction wiring) but store `pending_dispose = Some(cursor, Then::{Return, Throw, Complete})` in
   `AsyncFunctionState`. On resume with `pending_dispose` set, feed `Ok(v)`/`Err(e)` to the cursor
   *instead of* the normal `sent_value` handling; on `Done(completion)` resolve/reject the function promise
   and `remove_async_function_state`. `Completion::Exit` still propagates uncatchably.
6. **Red→Green (block-level)**: B2, B3 (add to the block file `await-using-block-dispose-tick-alignment.js`).
   Replace the `pending_async_dispose_await` flag with the same cursor: the `Statement::Block` arm, when the
   executor advertises it can suspend (new bool set by `async_function_resume` around
   `exec_state_machine_body`, **not** merely `in_state_machine`, which is also true for generator machines),
   builds the cursor from the block's own `completion` and `pending_dispose`-parks it instead of calling the
   blocking driver; after `exec_state_machine_body` returns, the executor checks the parked cursor, steps it
   and either suspends (`Then::Block { state: current_id }`) or continues. On resume, the finished completion
   is fed back as a **pre-loaded `stmt_result`** for the same state (the transform already ends that state
   with `Goto(resume_state)`, `generator_transform.rs:647-654`) so `Return`/`Break`/`Continue`/`Throw` from the
   block flow through the existing post-body handling unchanged. Delete `pending_async_dispose_await` and its
   three sites. Where no suspendable executor is active (any other `dispose_resources` caller) the blocking
   driver stays exactly as it is.
7. **Collapse duplication**: `dispose_resources` (`exec.rs:2666`) and the ADS copy both become wrappers over the
   cursor (blocking driver = loop on `await_value`). Only do this after 4–6 are green; blocking-driver await
   counts for un-migrated sites (§7) must be unchanged except that they now follow the spec's single trailing
   `Await` for null resources — call this out in the PR if any existing `test262-extra`/`tests` expectation moves.

Stop rule: if slice B does not fit the turn, land slice A as its own PR that says "Refs #645" (not "Fixes")
and post the remaining work on the issue; do not leave partial suspension machinery in-tree behind a flag.

## 5. Test surface

Targeted test262 runs (all must stay green, no baseline movement expected other than possible net gains):
* `test262/test/built-ins/AsyncDisposableStack/`, `test262/test/built-ins/DisposableStack/`,
  `test262/test/built-ins/SuppressedError/`
* `test262/test/language/statements/await-using/`, `test262/test/language/statements/using/`,
  `test262/test/language/statements/for-of/` and `for-await-of/` (the `await using`-in-head files),
  `test262/test/language/module-code/` (module-level `using`), `test262/test/language/expressions/async-*`
* `test262/test/staging/explicit-resource-management/` (run explicitly per `CLAUDE.md`)
* Full run `uv run python scripts/run-test262.py` at the end (the change reaches all async functions).
* `uv run python scripts/run-custom-tests.py` and `cargo test --release`; the existing
  `test262-extra/module-using-abrupt-completion-disposal.js` and the `#242` `__host_exit` cases in
  `src/interpreter/tests.rs:4160-4200` guard the `Completion::Exit` and abrupt-completion contracts.

Not covered by test262 (new `test262-extra/` tests, each pinning a DisposeResources step, see §3/§4):
tick alignment of `settled` vs a witness chain (steps 3.f/4), count-invariance for multiple nulls (step 4 is
*single*), step 3.d (`Await(undefined)` before a sync disposer after a null), pending-ness of the
`disposeAsync()` promise, and "no job runs inside synchronous code" for a null `await using`.

## 6. Regression risk

* **Every async function** goes through `async_function_resume` (`call_async_function` always builds a state
  machine), so slice B touches the hottest suspension path. Keep the new branches cold: the executor only
  consults `pending_dispose` when it is `Some`; functions without `await using` must not pay for it. Measure
  with the existing async benchmarks if any diff is visible in `perf-counters` output.
* **GC rooting / `gc_safepoint()`**: a suspended disposal owns `DisposableResource` values, the accumulated
  error and (ADS) the capability's promise + resolving functions across a microtask boundary. They must be
  reachable from the parked state: trace `AsyncFunctionState.pending_dispose` in `gc.rs`; for the ADS
  closure path root the promise/resolve/reject and the remaining stack via the `gc_root_frame`/
  `gc_root_value` pattern already used in `await_value` (`eval.rs:9583-9584`) or by capturing them in the
  reaction handlers' roots. A `test262-extra/` GC-stress case (allocate + `gc()` between the disposer call
  and its await) is required, following the existing `*-gc-rooting.js` files.
* **ObjectKind exhaustive match**: no new `ObjectKind` variant is planned; if a native-closure state object
  is added, it must be a plain captured closure, not an `ObjectKind`.
* **Baseline** (`test262-pass.txt`, read from `origin/main`): the five listed test262 files already pass and
  must keep passing. The change should not remove any passing test; net positives are possible in
  `staging/` and `for-await-of` tick-sensitive tests. Do not roll the baseline.
* **Generators / async generators**: `in_state_machine` is also true in generator machines, so the current
  flag can be left set by an async generator's `await using` block and consumed by an unrelated async
  function (suspected stale-flag bug; verify with a repro in slice 6 and note it in the PR if confirmed). The
  new suspendability bool must be set/cleared by `async_function_resume` only.
* **`Completion::Exit` (#242)**: preserve immediate, uncatchable propagation at every new step (cursor,
  reaction handlers, resume path).
* **Library harnesses**: `acorn`/`decimal.js` do not use `using`; no library run is needed, but run
  `scripts/run-library-tests.sh acorn` only if `async_function_resume` shows a non-trivial diff outside the
  dispose branches. Bytecode fast path: async functions are not on it (`bytecode_enabled` off by default);
  confirm with `grep` that no compiled path calls `dispose_resources`.

## 7. Out of scope (follow-up list — file as separate issues, do not bundle)

* `await using` in `for`/`for-of`/`for await`/`switch` heads and bodies (`exec.rs:1868-1937`, `2318`,
  `2505-2548`, `close_for_of_loop` `eval.rs:9324`, `unwind_async_for_of_loops`): these keep the blocking
  driver; their tick behavior stays as today.
* `await using` inside **async generators** (`generator_runtime.rs` dispose sites `:107-156`, `:1200-1257`,
  `:4020-4202`, `:4779-5117`) and top-level module `await using` (`mod.rs:3759`).
* Migrating the other blocking `await_value` callers (`exec.rs:2234`, `eval.rs:932/1018`,
  `generator_runtime.rs:3090…6111`) — same root cause class ("Await blocks instead of suspending"), much
  larger blast radius.
* The stale `pending_async_dispose_await`-flag question if it needs its own fix in async generators.
* Formatting, unrelated cleanups, and any change to `test262-pass.txt`.

PR title (squash subject): `fix(disposable): suspend at DisposeResources awaits instead of draining inline`
(if only slice A lands: `fix(disposable): settle AsyncDisposableStack.disposeAsync after its Await`).
