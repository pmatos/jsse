# Plan: issue #708 — `yield*` delegated steps in async generators use blocking `await_value`

Base: `main` @ eba6e183. All line numbers below are in
`src/interpreter/eval/generator_runtime.rs` at that commit unless stated.

## 1. Problem restated

In an async generator, only the *first* `Await(innerResult)` of a `yield*` is
suspended properly (`async_generator_next_state_machine_impl` ~4603-4764 builds
`PromiseResolve` + `PerformPromiseThen` handlers that call
`yield_star_await_inner_result_resume`, ~2654). Every later delegated step calls
the blocking `Interpreter::await_value` (`eval.rs:9874`), which runs a nested
microtask loop *inside the running job*:

| site | function / branch | line |
|---|---|---|
| return, request already queued | `yield_star_return_after_unwrap` — `Await(innerReturnResult)` | 3120 |
| return, generator suspended at `yield*` | `next_impl` delegated branch `stored_pending_return` | 3343 |
| throw, generator suspended at `yield*` | `next_impl` delegated branch `stored_pending_exception` | 3526 |
| next, 2nd+ step | `next_impl` delegated branch, plain `.next()` | 3740 |

Consequences, reproduced on the current build (`target/release/jsse`) and
cross-checked with node:

* **Reentrancy** — a reaction `B` on `p1` that calls `it.next()` / `it.throw()` /
  `it.return()` on a generator suspended in `yield*` runs reaction `C` (already
  queued on `p1`) *inside* `B`, before `B` returns.
  `B-start,B-end,C,…` (node) vs `B-start,C,B-end,…` (jsse), for all three kinds.
* **Pending inner result is read as `undefined`** — `await_value`'s loop `break`s
  when the microtask queue is empty and the promise is still pending (non-agent
  thread), returning `Completion::Normal(undefined)`. An inner `next()` / `return()`
  promise resolved by a *timer* makes the request settle immediately with
  `undefined` (`n2:true:undefined` / `ret:true:undefined` before the timer fires,
  and `after:undefined` for `var r = yield* it`). node waits for the timer.

The fix is to make every delegated step suspend the way the first step already
does, sharing one continuation.

### Repro scripts (jsse vs node; `print` = `console.log` under node)

```js
// next / throw / ret differ only in the call made from B (it.next() / it.throw('E') / it.return('R'))
var log = [];
async function* inner(){ yield 1; yield 2; yield 3; }
async function* g(){ yield* inner(); }
var it = g(); var p1 = it.next();
p1.then(function B(){ log.push('B-start'); it.next().then(function(r){ log.push('n2:'+r.value); }); log.push('B-end'); });
p1.then(function C(){ log.push('C'); });
setTimeout(function(){ print(log.join(',')); }, 20);
// jsse: B-start,C,B-end,n2:2      node: B-start,B-end,C,n2:2
// throw/ret variants use a hand-rolled inner iterator with next()/throw()/return() returning {value,done:false}:
//   jsse: B-start,C,B-end,thr:t:E   node: B-start,B-end,C,thr:t:E   (same shape for ret)
```
```js
// timer-gated inner result (2nd step)            jsse: n1:1,after:undefined,n2:true:undefined,release
var release, gate = new Promise(function(r){ release = r; }), n = 0;
var innerIt = { [Symbol.asyncIterator]() { return this; }, next() { n++; return n == 1 ? {value:1, done:false} : gate; } };
async function* g(){ var r = yield* innerIt; log.push('after:' + JSON.stringify(r)); }
var it = g(); it.next().then(...n1...); it.next().then(...n2...);
setTimeout(function(){ log.push('release'); release({value:'x', done:true}); }, 5);
//                                                node: n1:1,release,after:"x",n2:true:undefined
// queued return: `it.next(); it.return('R')` with inner.return() -> gate (timer-released, done:true)
//                                                jsse: ret:true:undefined (before release)   node: …release,ret:true:x
```

## 2. Spec basis

* `sec-generator-function-definitions-runtime-semantics-evaluation`,
  `YieldExpression : yield * AssignmentExpression` (spec.html:24261):
  * next step: "If _generatorKind_ is ~async~, set _innerResult_ to ? Await(_innerResult_)"
    followed by the not-an-Object TypeError, `IteratorComplete`, `IteratorValue`, and
    `AsyncGeneratorYield(? IteratorValue(_innerResult_))`;
  * throw step: same `? Await(_innerResult_)` for the inner `throw` result, `done` ⇒
    `Return ? IteratorValue`;
  * return step: `? Await(_innerReturnResult_)`, `done` ⇒
    `ReturnCompletion(_returnedValue_)`; no `return` method ⇒ `Await(receivedValue)` then
    `ReturnCompletion`.
* `await` (spec.html:51047, Await) — suspends the running execution context; the
  continuation runs as its own job via `PerformPromiseThen(promise, onFulfilled, onRejected)`
  on `PromiseResolve(%Promise%, value)` (`sec-promise-resolve`, `sec-performpromisethen`).
  It never runs other jobs inline.
* `sec-asyncgeneratoryield` — steps 9-11: complete the current request, then, if the
  queue is non-empty, continue *without suspending* into
  `sec-asyncgeneratorunwrapyieldresumption` (which itself `Await`s a return value).
* `sec-asyncgeneratorcompletestep` — resolve/reject the front request and remove it
  exactly once.

## 3. Files to touch

* `src/interpreter/eval/generator_runtime.rs` — all production changes.
* `test262-extra/` — new tests (section 5). No `tests/` additions needed (no
  host-diagnostic or stress behaviour).
* `docs/adr/2026-09-21-HHMM-yield-star-delegated-step-suspension.md` — short ADR
  (optional, last slice): records the `DelegateStep` seam and the two queue invariants below.
  Follows `docs/adr/2026-09-21-2015-async-generator-frame-exit-disposal.md`, which lists
  "blocking driver" boundaries; nothing in `CONTEXT.md` needs new vocabulary.
* Not touched: `eval.rs` (`await_value` stays for its other callers), `scheduler.rs`,
  `types.rs` (`IteratorState`/`DelegatedIteratorInfo` are unchanged — no `ObjectKind` edit,
  so no GC-walker change), `generator_transform.rs`, `test262-pass.txt`, `spec/`, `test262/`.

## 4. Design

### The two invariants (the whole regression surface)

1. **Flag set ⇒ hands off the queue.** A site that suspends sets
   `scheduler.set_async_gen_yield_pending(true)` and returns `Completion::Normal(promise)`.
   `async_gen_process_queue` (~2579-2650) then neither pops nor recurses; ownership of the
   front request passes to the continuation.
2. **The continuation settles/pops the front request exactly once**, then calls
   `async_gen_process_queue(gen_this)`. Double pop drops a request; no pop hangs the generator.
   Every exit of the continuation (rejection, TypeError, IteratorComplete/Value throw,
   done, not-done) must be audited against this.

The generator's `execution_state` stays `SuspendedAtState { resume_state }` with
`delegated_iterator: Some(..)` while parked (exactly what the first step stores), so a
request arriving during the await is queued (`queue_len > 1`) rather than started.

### Shared pieces

* **`await_then(value, on_fulfilled, on_rejected)`** (name TBD) — extraction of the
  duplicated block at ~2902-3034 and ~4607-4761: `promise_resolve_value` →
  `get_promise_state` → `{Fulfilled/Rejected → enqueue_microtask with roots
  vec![val, handler], Pending → is_handled = true + push two PromiseReactions,
  None → enqueue_microtask with the raw value}`. Preserve the microtask `roots` vector
  exactly (GC rooting).
* **`enum DelegateStep { Next, Throw, Return }`** and a
  `yield_star_suspend_on_inner_result(gen_this, gen_id, inner_result, step, promise, resolve_fn, reject_fn)`
  that builds the two native handlers (`yieldStarAwaitFulfill`/`Reject`), calls `await_then`,
  and sets the yield-pending flag. `yield_star_await_inner_result_resume` gains a `step`
  parameter. If clippy `too_many_arguments` trips, bundle `(promise, resolve_fn, reject_fn)`
  in a small struct (the fix must pass the `-D warnings` hook).
* **Resume behaviour by `step`** (rejection, non-object TypeError, `IteratorComplete` throw,
  and `IteratorValue` throw are identical for all steps — they already reject+complete /
  route through the try stack as today):
  * `done == false` — all steps: `AsyncGeneratorYield` tail already in the function
    (resolve request with `{value, done:false}`, pop, then queue Return → unwrap path /
    Next|Throw → `async_gen_process_queue` / empty → suspend).
  * `done == true`, `Next | Throw` — bind `IteratorValue` to the `yield*` result binding and
    resume the state machine at `resume_state` with `delegated_iterator: None`.
  * `done == true`, `Return` — complete the generator, `async_generator_await_return(value,
    promise_id)`, pop the request, `async_gen_process_queue` (the body of today's
    `yield_star_return_after_unwrap` `done` arm, 3178-3199).

### Divergences between the four old tails and the shared continuation (silent-bug risks)

* **Binding source.** The `.throw()` tail (3613) and `.next()` tail (3828) read the binding from
  `deleg_info.sent_value_binding`; the resume function reads the state's `pending_binding`
  (2809). The converted sites must store `pending_binding: binding` when parking (the first step
  does: `pending_binding: sent_value_binding.clone()`), or the resume function must read
  `deleg_info.sent_value_binding`. The two also bind differently: `.next()` tail uses
  "`initialize_binding` if uninitialized else `set`"; resume uses `env_set(..).ok()`. A
  `const r = yield* it` / `let r; r = yield* it` test at every step kind pins this — both
  currently work (checked: first-step-done and later-step-done both give `r=V`), so extract one
  `bind_yield_star_result(func_env, binding, value)` helper using the tolerant logic and use it
  from both.
* **`IteratorValue` throw routing.** `.return()`/`.throw()` tails and the resume function park
  at `resume_state` with `pending_exception` (routes through the try stack). The `.next()` tail
  instead truncates `try_stack` and jumps to `catch_state`; on the current build this is
  *wrong*: `try { yield* it } catch(e){ yield 'caught:'+e }` with a 2nd-step
  `{done:false, get value(){throw}}` leaves the request rejected (`next2-threw:vget2`), while
  node and the first step catch it. Converging on the resume function fixes it as a side
  effect; pin it with a test and call it out in the PR body.
* **Sent value on done.** `.next()`/`.throw()` tails resume with `sent_value = undefined`;
  the resume function resumes with `value`. Irrelevant for `Variable`/`Pattern`/`Discard`
  bindings; check `SentValueBindingKind::InlineYield` (the degraded fallback,
  `generator_runtime.rs` / `generator_transform.rs`) before settling which one wins, and keep
  the first step's behaviour unless a test shows otherwise.
* **Return-mode `done` uses `async_generator_await_return`**, which itself calls
  `drain_microtasks()` twice (6627, 6660). Out of scope (section 7), but it means Return-mode
  *ordering* tests must use an inner result with `done:false` (or assert only final values for
  `done:true`), so they observe the `Await(innerReturnResult)` and not the later drains.

### Behaviour deliberately preserved

* Rejection of the awaited inner result rejects the request and completes the generator
  (`is_rejection` branch, 2688-2703). All four old sites already did exactly this, so
  converging is neutral on the known try/catch gap below.
* The pre-await "not an object" checks inside `iterator_return`/`iterator_throw` and the
  `.next()` call check stay where they are (only timing differs from spec; out of scope).
* Synchronous failures before any Await (getting `next`/`return`/`throw`, calling them,
  non-callable) keep their current reject+complete tails.

## 5. TDD slices

Test runner for every slice: `cargo build --release -j4` (nothing else builds while a suite
runs) then `uv run python scripts/run-test262.py test262-extra/<file>` for the new file.
Run gates as separate commands (build, clippy hook, test) — never `&&`-chained.
First `git submodule update --init --depth 1 test262 spec` in a fresh workspace.

All new files live in `test262-extra/`, follow
`async-generator-await-using-block-dispose-tick-alignment.js` (copyright header,
`/*--- esid, description, info, flags: [async], includes: [asyncHelpers.js, compareArray.js],
features: [async-iteration] ---*/`, `asyncTest(async function(){…})`), use
`esid: sec-generator-function-definitions-runtime-semantics-evaluation`, and quote the relevant
`yield*` steps plus `sec-asyncgeneratoryield` / `sec-asyncgeneratorunwrapyieldresumption` in
`info:`. Ordering assertions never count ticks: they compare log *order* (`B-start, B-end, C`
then a later marker) and `await` the promises returned by the nested request.

1. **Refactor: extract `await_then`** (no test — existing suite is the proof).
   Replace the two duplicated blocks (~2902-3034 unwrap-return; ~4607-4761 first step) with
   calls to the helper. Gate: `test262-extra/async-generator-*`, and
   `test262/test/language/{expressions,statements}/async-generator/`,
   `test262/test/built-ins/AsyncGeneratorPrototype/` byte-identical results vs. baseline.
2. **Refactor: `DelegateStep` + `yield_star_suspend_on_inner_result`**, first step converted
   (`step = Next`); resume function takes `step` but only `Next` is wired; extract
   `bind_yield_star_result`. No behaviour change; same gate as slice 1.
3. **Red → green: delegated `.next()`, 2nd+ step.**
   * `async-generator-yield-star-next-step-await-does-not-nest.js` — the `B`/`C` repro with
     `async function* inner(){yield 1; yield 2; yield 3}` (assert `['B-start','B-end','C']`
     precedes `'n2:2'`; assert values `1,2,3,done`). Also queue several requests while a
     later step is parked (`it.next(); it.next(); it.next(); it.return('R')` with an async inner
     iterator whose `next()` returns a promise) and assert FIFO settlement — pins invariants 1-2.
   * `async-generator-yield-star-later-step-pending-inner-result.js` (part 1) — timer-released
     inner `next()` promise: the request must settle only after release with the released value
     (`after:"x"`, `n2:true:undefined`), never `undefined` early. `setTimeout` precedent:
     `async-generator-await-using-dispose-suspended-gc-rooting.js`.
   * `async-generator-yield-star-later-step-result-binding-and-value-throw.js` — `const r =
     yield* it`, `let r; r = yield* it`, destructuring binding, with `done:true` at step 1 and
     step 2; plus `try { yield* it } catch(e){ yield 'caught:'+e }` with a 2nd-step
     `get value(){throw}` (red today: rejected with `vget2`; green after — the incidental fix).
   Production: replace the `.next()` tail (3725-3925) — keep the call/type checks and error
   tails, then park state (`delegated_iterator: Some`, `pending_binding: binding`) and call
   `yield_star_suspend_on_inner_result(.., Next, ..)`; delete the inline
   `await_value`/`iterator_complete`/`iterator_value`/binding/resolve tail.
4. **Red → green: delegated `.throw()`.**
   `async-generator-yield-star-throw-step-await-does-not-nest.js` (inner `throw` returns
   `{value, done:false}`; same `B`/`C` shape; also `done:true` ⇒ generator continues with the
   bound value, and a `get value(){throw}` under try/catch); add the timer-gated case to the
   `later-step-pending-inner-result` file. Production: convert the `stored_pending_exception`
   `Ok(Some(..))` arm (3524-3691). `Ok(None)` (no `throw` method) and `Err` arms unchanged.
5. **Red → green: delegated `.return()`, generator suspended at `yield*`.**
   `async-generator-yield-star-return-step-await-does-not-nest.js` (inner `return` gives
   `{done:false}` for the ordering assertion; a second `done:true` case asserts only the final
   `{value, done:true}` and settlement order, not ticks) and a timer-gated case in the pending
   file. Production: add the `Return` `done` arm to the resume function; convert the
   `stored_pending_return` `Ok(Some(..))` arm (3341-3484). `Ok(None)` arm unchanged.
6. **Red → green: return request already queued (`yield_star_return_after_unwrap`).**
   `async-generator-yield-star-queued-return-pending-inner-result.js`: `it.next();
   it.return('R')` with inner `return()` → timer-released promise; today
   `ret:true:undefined` before the release, node `ret:true:x` after. Production: replace the
   blocking await + tail in `yield_star_return_after_unwrap` (3117-3224) with
   `yield_star_suspend_on_inner_result(.., Return, ..)`; state is already parked with
   `delegated_iterator: Some` and the Return request stays at the queue front.
   After this slice `grep -n await_value` shows none in the four yield* delegation functions.
7. **Guard: GC while parked at a later-step await.**
   `async-generator-yield-star-later-step-await-gc-rooting.js`
   (`features: [async-iteration, host-gc-required]`, pattern of
   `async-generator-await-using-dispose-suspended-gc-rooting.js`): `$262.gc()` while a
   2nd-step inner promise is pending and the generator/request promise are otherwise
   unreferenced from JS; release afterwards; assert the value and the request promise survive.
   May be green on arrival — it is a regression guard for the new suspension window. If it
   fails, the native handler captures are not rooted for this path; root them the way #706 roots
   pending requests, not by special-casing the test.
8. **Cleanup + record.** Remove now-dead code, run the linter hook, add the short ADR
   (`DelegateStep` seam; the two invariants; the remaining blocking boundaries from
   section 7).

## 6. Test surface and regression risk

Targeted test262 (run before the full suite; `uv run python scripts/run-test262.py <dir>`):

* `test262/test/language/expressions/async-generator/` and
  `test262/test/language/statements/async-generator/` (`yield-star-*`, `yield-promise-reject-next-yield-star-*`)
* `test262/test/language/expressions/yield/` (`star-rhs-iter-*`, async variants)
* `test262/test/built-ins/AsyncGeneratorPrototype/{next,return,throw}/`
* `test262/test/built-ins/AsyncGeneratorFunction/`, `test262/test/built-ins/AsyncFromSyncIteratorPrototype/`
* class / object-method async-generator dirs under `language/statements/class/`,
  `language/expressions/class/`, `language/expressions/object/method-definition/` for
  `yield-star-*` names
* then the full suite (`uv run python scripts/run-test262.py`) and `uv run python
  scripts/run-custom-tests.py`; `cargo test --release`.

Not in test262 (⇒ `test262-extra/` files in section 5): the reentrancy/ordering guarantee of a
non-blocking delegated `Await`, timer-released inner results, request-queue FIFO while a later
step is parked, binding kinds at later steps, the incidental try/catch fix, and GC rooting.

Regression risk:

* `test262-pass.txt` carries ~1,440 `yield-star` entries; microtask *ordering* around the
  delegated steps changes (nested → sequential), so tick-sensitive `yield-star-*` /
  `AsyncGeneratorPrototype` tests are the ones that could flip. Expect zero regressions (the
  suite is spec-aligned and the first step already behaves this way); any flip is investigated
  against the spec, not patched around. The baseline is read from `origin/main` — do **not**
  run `--update-baseline`.
* Shared machinery leaned on: `async_gen_process_queue` and the global (not per-generator)
  `scheduler.async_gen_yield_pending` flag — the new suspension paths must set it on every
  parked return and clear it consistently (2596, 2635, 2770, 2837 already do); the async-
  generator request queue (rooting per #706, popped exactly once); the `IteratorState::
  StateMachineAsyncGenerator` fields (`pending_binding`, `delegated_iterator`,
  `pending_exception`, `pending_return`) — unchanged shape, so no exhaustive-`ObjectKind`
  match or GC-walker edit; `generator_inline_iters` cleanup on the terminal paths.
* Not affected: the tree-walker hot paths (`eval_expr` / `exec_statement`), the property MOP,
  the bytecode fast path (async generators run on the state machine), and the Node-compat
  library harnesses. Smoke-check `./scripts/run-library-tests.sh acorn` only if the full suite
  shows any async-iteration movement (highlight.js/uglify-js are too slow to be worth it).

## 7. Out of scope (record in the PR body / file follow-ups, do not fix here)

Observed on the current build while planning (node behaves per spec in each):

* **`finally` not run when `.return()` unwinds a delegated `yield*`** — return completion
  isn't routed through the try stack (`try { yield* it } finally { log }` logs nothing).
* **Rejected inner `next()` isn't catchable by the generator's own `try/catch`** — the
  `is_rejection` branch (2688-2703) rejects the request and completes the generator. Same for
  the four old sites, so this change is neutral on it.
* **Direct delegated `.return(v)` skips `AsyncGeneratorUnwrapYieldResumption`'s `Await(v)`** —
  the `stored_pending_return` branch calls `iterator_return` immediately (thenable `then` getter
  never read; node reads it first).
* Pre-`Await` "not an object" checks in `iterator_return` / `iterator_throw` / the `.next()`
  call (spec checks after the Await; timing-only).
* `iterator_close` in the missing-`throw` path is not `AsyncIteratorClose` (no Await).
* Remaining blocking waits / inline drains: `drain_microtasks()` inside
  `async_generator_await_return` (6627, 6660) and after settling on many error paths of
  `async_generator_next_state_machine_impl`; `await_value` for the `yield` operand (4422),
  for-await step (5717), legacy `IteratorState::AsyncGenerator` path (6503, 6541).
* No formatting-only or unrelated cleanup of `generator_runtime.rs`; no baseline changes.
