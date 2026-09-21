# Plan: issue #712 — async-generator queue driver drains microtasks after settling

Baseline: `origin/main` = `2acae0e3`. All line numbers below are for that commit, in
`src/interpreter/eval/generator_runtime.rs` unless stated otherwise.

**Status of this plan: the design has been prototyped end to end in a throwaway worktree**
(outside the repo) and measured — see "Prototype evidence". The implementation stage is
reproducing a known-green change, not exploring.

## 1. Problem restated

The state-machine async-generator queue driver (`async_gen_enqueue` → `async_gen_process_queue`
→ `async_generator_next_state_machine_impl` and the `…_return_/_throw_state_machine_with_promise`
wrappers) settles a request's promise and then calls `drain_microtasks()` before handing the
promise back to a *synchronous* caller (31 sites in the impl, 3 in the return/throw wrappers, 2 in
`async_generator_await_return`). The drain runs every queued job inside `it.next()/return()/throw()`,
which is observable (`job` runs before the call returns; spec says settling only enqueues jobs).
The drains were left in by #687 because `async_generator_await_return` is not spec-shaped: it
settles the request one microtask *after* `async_gen_process_queue` has already popped the request
and started the next one, so without the drain later requests overtake an earlier `return()`.
This is a real bug independent of the drain: with a **pending** `return(operand)`, jsse lets a
following `next()`/`throw()` settle *before* the return (measured: `next:undefined:true` is logged
while the operand is still pending; node/spec log nothing until it settles), and on some paths
(`yield*` inner `return()` completing with a pending value, `await using` at a return) the return
never settles at all. Related: `async_generator_await_return` builds its wrapper with a resolving
function instead of `PromiseResolve(%Promise%, value)`, costing two extra jobs for a native-promise
operand (hidden today by the drain).

Fix: implement AsyncGeneratorAwaitReturn as the spec does — keep the request at the head of the
queue (this *is* the `draining-queue` state) until the PerformPromiseThen reaction completes it
and drains the rest of the queue — then delete the drains.

## 2. Spec basis

`spec/spec.html` (tc39/ecma262 @ submodule pin `270a490b`; the text here still names the state
`draining-queue`, the issue's "awaiting-return" is the same state). Clause numbers follow the
existing in-code citations (§27.6.3.x); ids are the `emu-clause` ids.

- `sec-asyncgenerator-prototype-next` / `-return` / `-throw` (§27.6.1.2–4): a request made while the
  state is `executing` or `draining-queue` is only **enqueued** (`next` step 8, `return` step 9,
  `throw` step 9); `return` on `suspended-start`/`completed` sets `draining-queue` and performs
  AsyncGeneratorAwaitReturn; `next`/`throw` on `completed` settle immediately without enqueueing.
- `sec-asyncgeneratorenqueue` (§27.6.3.4) and the request-record table (§27.6.3.1): "[[AsyncGeneratorQueue]]
  is non-empty if and only if [[AsyncGeneratorState]] is either executing or draining-queue" — the
  invariant that lets the retained queue head stand in for the state.
- `sec-asyncgeneratorcompletestep` (§27.6.3.5): removes the head, *then* calls the capability's
  resolve/reject (which only enqueues jobs — the drains have no basis here).
- `sec-asyncgeneratoryield` (§27.6.3.8) and `sec-asyncgeneratorunwrapyieldresumption` (§27.6.3.7):
  continue with the next queued request without suspending.
- `sec-asyncgeneratorawaitreturn` (§27.6.3.9): `PromiseResolve(%Promise%, value)`; on abrupt
  completion CompleteStep(reject) + DrainQueue **synchronously**; otherwise a single
  `PerformPromiseThen(promise, onFulfilled, onRejected)` whose closures do CompleteStep then
  AsyncGeneratorDrainQueue.
- `sec-asyncgeneratordrainqueue` (§27.6.3.10): serves queued normal/throw requests in the same job,
  starts another AwaitReturn at the next return request, else sets `completed`.
- `sec-promise-resolve` (§27.2.4.7.1) and `sec-performpromisethen` (§27.2.5.4.1): a native promise
  whose `constructor` is `%Promise%` is returned unchanged (no NewPromiseResolveThenableJob,
  `sec-newpromiseresolvethenablejob`), and PerformPromiseThen with no result capability queues
  exactly one job for an already-settled promise.

## 3. Design

No new `StateMachineExecutionState` variant. `async_gen_enqueue` already starts a request only when
`!executing && queue_len == 1`, and `Scheduler::for_each_root` already roots the generator plus
every queued request. Retaining the head across the await therefore *is* `draining-queue`
(the same technique ADR-2026-09-21-2015 uses for parked disposal). A new variant would touch every
exhaustive `StateMachineExecutionState` match (74 uses in `generator_runtime.rs`, 8 elsewhere) for
no observable change; the ADR records this choice.

New private API in `generator_runtime.rs`, replacing `async_generator_await_return`:

```rust
enum AwaitReturnStart { Parked, Settled }

fn async_gen_await_return(
    &mut self,
    gen_this: &JsValue,
    value: JsValue,
    request: (&JsValue, &JsValue, &JsValue), // (promise, resolve_fn, reject_fn), as async_gen_dispose takes
) -> AwaitReturnStart
```

1. `PromiseResolve(%Promise%, value)` via `promise_resolve_with_constructor(&Promise, &value)`
   (same lookup convention as the existing pre-check at ~:6218). `value` must be rooted across it
   (`with_gc_root_scope` + `gc_root_value`, as `await_then` does) — it can run user code
   (`constructor` getter, capability constructor) and therefore collect.
2. `Err(e)` → call `reject_fn(e)` and return `Settled` (spec: CompleteStep(reject) now). The head is
   **not** popped; the caller finishes as for any synchronous settle.
3. `Ok(p)` → `perform_promise_then(&p, onFulfilled, onRejected, UNDEFINED, UNDEFINED, UNDEFINED)`
   (no result capability, exactly what `schedule_await_resume` does) and return `Parked`.
   Both closures capture `gen_this`, `gen_id`, and the request's resolve/reject; each does, **in this
   order**: `resolve(iterResult(v, true))` / `reject(e)` → `queue.pop_front()` →
   `async_gen_process_queue(&gen_this)`. Settling before popping matters: `resolve` reads
   `iterResult.then`, which user code can define, and a re-entrant `next()` from there must still
   see a non-empty queue (spec: still `draining-queue`) and be merely enqueued.
   The closures are opaque to the tracer; that is safe only because the head request stays queued
   and `for_each_root` roots the generator and the request's functions.
4. The helper must **not** touch `set_async_gen_yield_pending`. The flag is a process-global that
   continuation-context callers never reset; setting it from the helper would leak into an
   unrelated driver invocation. Each caller acts on the return value instead:

   | Caller (baseline line) | Context | On `Parked` | On `Settled` |
   |---|---|---|---|
   | impl, delegated pending-return with no `return` method (:3342) | driver | `set_async_gen_yield_pending(true)`; `return Completion::Normal(promise)` | `return Completion::Normal(promise)` |
   | impl, `Completion::Return` after `dispose_or_park!(…ReturnAwait)` (:3793) | driver | same | same |
   | `async_generator_return_state_machine_with_promise`, suspended-start/completed (:6210) | driver | same | same |
   | `yield_star_await_inner_result_resume`, inner return done (:2970) | continuation | `return` (reaction owns pop + drain) | existing pop + `async_gen_process_queue` |
   | `yield_star_return_after_unwrap`, no `return` method (:3214) | continuation | `return` | existing pop + process |
   | `async_gen_finish_disposal`, `ReturnAwait` (:5726) | continuation | `return Completion::Normal(UNDEFINED)` before the unconditional pop/process at its tail | fall through to it |

   Every driver-context caller is reached from `async_gen_process_queue` or from a continuation that
   already tests `is_async_gen_yield_pending()` after `async_generator_next_state_machine_with_promise`
   (`:2924`, `:2997`, `:5765`, `:5836`), so the existing protocol carries `Parked` upward unchanged.

Then delete `drain_microtasks();` at: every site inside `async_generator_next_state_machine_impl`
(31, baseline lines 3354–5374), `async_generator_return_state_machine_with_promise` (the `Executing`
arm, :6198), and `async_generator_throw_state_machine_with_promise` (`Executing` arm :6304 and the
`SuspendedStart | Completed` arm :6317). Result: `drain_microtasks` in `generator_runtime.rs` 44 → 10 (8 if #730 has removed the legacy
functions first). The issue's "~40 sites" folds in legacy-path drains that #730 owns; the in-scope
count is 34 (31 + 3).

**Keep untouched:** `reject_with_type_error` (:2583), the eval.rs parameter-binding drain, and
everything in the dead legacy `IteratorState::AsyncGenerator` functions (they belong to #714 / #730;
:6317 is also #714's — deleting the same line here is an identical hunk and merges cleanly either
way, and is needed for `throw` on a completed generator not to drain).
The legacy callers of the old `async_generator_await_return` (:6396, :6413) are unreachable;
`git fetch origin main` first: if #730 has landed they are gone and the old function is simply
deleted; if not, rename the old function `async_generator_await_return_legacy` byte-for-byte and let
#730 delete it (no behaviour change to dead code, no conflict with #730).

## 4. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the only production file (new helper + enum, six
  call sites, drain deletions; drop the now-unused `promise_id` local in
  `async_generator_return_state_machine_with_promise`, otherwise the clippy `-D warnings` hook blocks).
- `test262-extra/` (new, see Appendix A for validated sources):
  - `async-generator-await-return-parks-later-requests.js`
  - `async-generator-await-return-promise-resolve-ticks.js`
  - `async-generator-await-return-pending-gc-rooting.js`
  - `async-generator-requests-do-not-drain-microtasks.js`
  - `async-generator-request-queue-return-throw-next-fifo.js` — **the issue says this guard is "already
    in the tree"; it is not.** It exists only on the open #714 branch (commit `61799f65`, branch
    `sym/jsse/687-bug-blocking-await-value-callers-drain-the-microtask-queue-inline-eval-exec-generator-runtime`).
    If `origin/main` lacks it, add it byte-identical
    (`git show 61799f65:test262-extra/async-generator-request-queue-return-throw-next-fifo.js`) so
    whichever PR lands second sees an identical add.
- `docs/adr/2026-09-22-HHMM-async-generator-awaiting-return-parking.md` (new; UTC timestamp at
  authoring time, per `docs/adr/README.md`): decisions = queue head is the `draining-queue` marker
  (no new state variant), reaction owns settle→pop→drain, helper returns `Parked|Settled` and never
  touches the global yield-pending flag, drains deleted. Cross-link ADR-2026-09-21-2015 and
  ADR-2026-09-21-2300 (its "Known boundaries" line saying `async_generator_await_return` drains is
  historical and stays as is).
- `.architecture/backlog.md` — one dated line under `settle-and-return-tail`: the canonical tail is
  now "settle + return" with no drain, shrinking that candidate to a pure call-shape extraction.
- No changes to `scheduler.rs`, `gc.rs`, `types.rs`, `mod.rs`, `CONTEXT.md`. `test262-pass.txt` is
  never rewritten. `CHANGELOG.md` is release-generated.

## 5. TDD slices

Setup (no code): `git fetch origin main`; if `origin/main` moved (esp. #714 or #730 merged), rebase
this branch first. `git submodule update --init --depth 1 spec test262`.
`CARGO_PROFILE_RELEASE_DEBUG=0 cargo build --release -j8` (~1 min); keep that binary as the
baseline binary (copy to `$TMPDIR`) for red checks — never rebuild over a running suite.

1. **Red: pending return must park later requests.** Add
   `async-generator-await-return-parks-later-requests.js`. Confirm it fails on the baseline binary
   (a following `next`/`throw`/`return` settles while the operand is pending; two `yield*`/`await using`
   variants never settle). Green by slice 2.
2. **Red: PromiseResolve tick shape.** Add `async-generator-await-return-promise-resolve-ticks.js`
   (`return(Promise.reject('E'))` at suspended-start must log `sync-end, job, w1, rejected, w2…`;
   fulfilled native promise: one job; thenable: two). Fails on baseline. Green by slice 3.
3. **Green for 1–2: the AwaitReturn helper.** Add `AwaitReturnStart` and `async_gen_await_return`;
   switch the six state-machine call sites per the §3 table; handle the legacy callers per §3; drop
   the unused local. Drains elsewhere are still present in this slice. Run `./scripts/lint.sh` now
   (rustc alone does not catch what clippy `-D warnings` — which also gates every `.rs` edit via the
   PostToolUse hook — will say about the new two-variant enum compared with `==` and the
   three-reference tuple parameter; the prototype never ran clippy). Then run
   `test262/test/built-ins/AsyncGeneratorPrototype`, `language/{statements,expressions}/async-generator`,
   `for-await-of`, `yield`, and all of `test262-extra/`.
   *If a remaining drain makes this slice regress on its own, fold slice 5 into it — the prototype
   applied both together and was fully green.*
4. **Red: GC rooting of the parked request.** Add `async-generator-await-return-pending-gc-rooting.js`
   (`$262.gc()` twice while parked on a pending operand with a queued `next` and `throw`). Fails on
   baseline, green with slice 3 (head retention roots generator + requests). If it goes red in
   slice 3, the closures/`value` are under-rooted — fix rooting, do not weaken the test.
5. **Red: no inline drain.** Add `async-generator-requests-do-not-drain-microtasks.js` (24 scenarios,
   one per drain family: completed/executing entry, body throw, resume-into-throw, try/catch/finally,
   while condition, switch discriminant/case test, for-of iterable/step, `yield*` next/return/throw
   failures and non-object result, return/throw at every state, `await using` disposer throws) and
   the FIFO guard. Then delete the 34 drains per §3. Green. `grep -c drain_microtasks
   src/interpreter/eval/generator_runtime.rs` → 10 if the old function was kept as
   `async_generator_await_return_legacy` (`reject_with_type_error` + the legacy paths, including that
   function's 2 drains), or 8 if #730 landed and the function was deleted outright. Either is correct.
6. **Refactor/docs.** ADR + backlog line; `./scripts/lint.sh` once more; PR title
   `fix(generators): park async-generator requests at AwaitReturn instead of draining microtasks`,
   body `Closes #712`, noting #714 (owns the 3 remaining independent drains and the guard test) and
   #730 (deletes the legacy functions).

Commit each slice green (test + fix together where the test only passes with the fix); the PR is
squash-merged, so history inside it is free but should stay bisectable.

## 6. Test surface

Targeted test262 (all must stay 100%, prototype: 6,076/6,076 for these):
`built-ins/AsyncGeneratorPrototype/{next,return,throw}` (especially `request-queue-order*`,
`request-queue-await-order`, `request-queue-promise-resolve-order`, `return-suspendedStart*`,
`return-state-completed*`, `return-suspendedYield*`), `built-ins/AsyncGeneratorFunction`,
`built-ins/AsyncFromSyncIteratorPrototype`, `built-ins/Promise`,
`language/{statements,expressions}/async-generator`, `language/statements/for-await-of`,
`language/expressions/{yield,await,class,object}`, `language/statements/{class,for-of,for-in,using,await-using}`,
`built-ins/{Array/fromAsync,AsyncDisposableStack,AsyncIteratorPrototype}`. Then the full default
run (`uv run python scripts/run-test262.py -j 12`, expect 99,911/99,911, 0 regressions, 0 new
passes), `uv run python scripts/run-test262.py test262-extra/`, `uv run python scripts/run-custom-tests.py`,
`cargo test --release`, `./scripts/lint.sh`.

**Not covered by test262, hence `test262-extra/`** (each cites its clause in `esid`):
`sec-asyncgeneratorawaitreturn` (parking; tick shape; GC rooting), `sec-asyncgeneratorcompletestep`
(drain-free settle), `sec-asyncgeneratorenqueue`/`-drainqueue` order (guard). test262's own
request-queue tests pass on the baseline because the drain hides the bug.

Reference probes (baseline jsse vs node vs prototype) used to choose scenarios: of 22 first-order
request scenarios, 14 ran `job` inside the call on baseline (12 in scope; the other two —
invalid receiver, throw at start — are #714's; all but invalid-receiver match node after the
prototype); of 21 further scenarios spanning every drain family, 13 were red on baseline and 12
are fixed (the 13th is `for await`, see §7); 5/5 pending-return-then-`next` scenarios diverged on baseline (2 never settled) and
match node after.

## 7. Regression risk

- **Baseline (`test262-pass.txt`) should not move**: prototype = 99,911/99,911, 0 regressions,
  0 new passes; `test262-extra` 567/567 before adding the new files; custom 15/15; `cargo test
  --release` green. Do not pass `--update-baseline`.
- **Global microtask interleaving.** Removing 34 drains changes when jobs run relative to the calling
  script for *any* code mixing async generators with other promise jobs. Hidden order-dependence in
  `test262-extra/*` or `tests/*` would show up as ordering failures there (none in the prototype).
- **`async_gen_yield_pending` protocol** (`scheduler.rs`, process-global flag): correctness of the
  driver/continuation table in §3 is the crux. A `Parked` result that is dropped pops a still-parked
  request; a helper that sets the flag itself leaks it. The slice-1/5 tests exercise every context.
- **GC rooting**: the two new native closures capture `gen_this`/resolve/reject and are untraceable;
  safe only while the head stays queued (`Scheduler::for_each_root`). Slice 4 pins it.
- Shared machinery leaned on but not modified: `promise_resolve_with_constructor`,
  `perform_promise_then`, `create_iter_result_object`, `async_gen_process_queue`, `pending_exit`
  latching. Not touched: tree-walker hot paths (`eval_expr`/`exec_statement`), the property MOP,
  `ObjectKind` matches, bytecode fast path, Node-compat harnesses (no async-generator dependence;
  run `./scripts/run-library-tests.sh zod` only if CI points there).
- **Known residual, deliberately left red:** a request that hits `for await` step errors still runs a
  job inline (`await_value` blocking calls at baseline :3995 and :5183 — tracked by #707 for `for await` and
  #710 for the inline-yield fallback, both under #687). The new
  tests must not include `for await` / inline-yield scenarios (the prototype probe row
  `for-await-next-throws` stays `job,sync-end` where node gives `sync-end,job`).

## 8. Out of scope

- The other #687 blocking `await_value` callers (`:5183` `for await` → #707; `:3995` inline-yield fallback → #710) and the three independent drains
  owned by #714 (`reject_with_type_error`, eval.rs parameter binding; the throw-at-start line is
  also deleted here only because the `throw: completed` scenario requires it).
- Deleting the dead legacy `IteratorState::AsyncGenerator` paths (#730).
- Extracting the "settle + return promise" tail into one helper (`.architecture/backlog.md`
  `settle-and-return-tail`); this PR deletes lines, it does not restructure them.
- Replacing the `get_global_var("Promise")` lookup with an intrinsic `%Promise%` accessor (none
  exists; `promise_resolve_value`/`await_then` use the same convention) and the async-from-sync,
  `yield*`, and `for await` tick-accounting differences not named in the issue.
- Moving the AsyncGeneratorUnwrapYieldResumption `Await` relative to `finally` blocks, and the
  `async_gen_await_resume` microtask-deferred queue advance (~:5861).
- The `Executing` arms of the impl and the return/throw wrappers (:3460, :6195, :6301) reject with a `TypeError`, while
  the spec (`%AsyncGeneratorPrototype%.next` step 8, `.return` step 10, `.throw` step 9) *enqueues* a
  request made while `executing`/`draining-queue`. They appear unreachable — `async_gen_enqueue`
  only starts a request when `!executing && queue_len == 1`, and the three
  `request-queue-order-state-executing.js` tests pass — so their drains are deleted only for
  uniformity; this PR neither endorses nor changes the TypeError.
- Formatting-only or unrelated cleanups.

## Prototype evidence (throwaway worktree, not in the repo)

Applied exactly §3 (helper + six call sites + 34 deletions) on `2acae0e3`:
`AsyncGeneratorPrototype`/async-generator/for-await/yield/await/Promise dirs 6,076/6,076;
class/using/await-using/fromAsync/AsyncDisposableStack/object/for-of/for-in 21,351/21,351; full
default run 99,911/99,911 with 0 regressions and 0 new passes; `test262-extra` 567/567;
custom tests 15/15; `cargo test --release` all green. Each of the four new tests below (2 scenarios each: sloppy + strict)
is red on the baseline binary and green on the prototype. Issue examples on prototype:
`it.return(3)…; it.next()…` → `ret,next`; `return(1); throw('q'); next()` on a completed generator →
`return,throw,next`; `return(Promise.reject('E'))` at suspended-start → `sync-end,job,w1,rej,w2,w3,w4`
(node-identical).

## Appendix A — validated test sources


### `test262-extra/async-generator-await-return-parks-later-requests.js`

```js
// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  While AsyncGeneratorAwaitReturn waits for its operand, the generator is in
  the draining-queue state: later requests wait behind it and are served, in
  order, by AsyncGeneratorDrainQueue once the operand settles.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  [...]
  9. Let onFulfilled be CreateBuiltinFunction(fulfilledClosure, 1, "", « »).
  [...]
  13. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  fulfilledClosure performs AsyncGeneratorCompleteStep and then
  AsyncGeneratorDrainQueue; until then the request stays at the head of the
  queue, so a request made meanwhile is only enqueued (%AsyncGeneratorPrototype%.next
  step 8, .return step 9, .throw step 9).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, async-explicit-resource-management]
---*/

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 10; i++) p = p.then(function () {});
  return p;
}

function inner(next, ret) {
  var it = {
    next: next || function () { return { value: 1, done: false }; },
    return: ret
  };
  it[Symbol.asyncIterator] = function () { return this; };
  return it;
}

var starts = {
  'suspended-start': [function () { return (async function* () {})(); }, null],
  completed: [function () { return (async function* () {})(); }, function (it) { return it.next(); }],
  'suspended-yield': [function () { return (async function* () { yield 1; })(); }, function (it) { return it.next(); }],
  'yield in try-finally': [function () { return (async function* () { try { yield 1; } finally {} })(); }, function (it) { return it.next(); }],
  'yield* with return() completing the delegation': [function () {
    return (async function* () { yield* inner(null, function (v) { return { value: v, done: true }; }); })();
  }, function (it) { return it.next(); }],
  'yield* without return()': [function () { return (async function* () { yield* inner(); })(); }, function (it) { return it.next(); }],
  'await using at the return': [function () {
    return (async function* () { await using d = { async [Symbol.asyncDispose]() {} }; yield 1; })();
  }, function (it) { return it.next(); }]
};

var followers = {
  next: ['next', 'next:undefined:true'],
  throw: ['throw', 'throw-rejected:x'],
  return: ['return', 'return:x:true']
};

asyncTest(async function () {
  for (var name in starts) {
    for (var kind in followers) {
      var it = starts[name][0]();
      if (starts[name][1]) await starts[name][1](it);
      await flush();

      var release;
      var pending = new Promise(function (resolve) { release = resolve; });
      var log = [];
      var first = it.return(pending).then(function (r) { log.push('first:' + r.value + ':' + r.done); });
      var second = it[kind]('x').then(
        function (r) { log.push(kind + ':' + r.value + ':' + r.done); },
        function (e) { log.push(kind + '-rejected:' + e); }
      );

      await flush();
      assert.compareArray(log, [], name + ' / ' + kind + ': nothing settles while the operand is pending');

      release(5);
      await Promise.all([first, second]);
      assert.compareArray(
        log,
        ['first:5:true', kind === 'next' ? 'next:undefined:true' : kind === 'throw' ? 'throw-rejected:x' : 'return:x:true'],
        name + ' / ' + kind + ': the return settles first, then the later request'
      );
    }
  }
});
```

### `test262-extra/async-generator-await-return-promise-resolve-ticks.js`

```js
// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  AsyncGeneratorAwaitReturn runs PromiseResolve(%Promise%, value) and a single
  PerformPromiseThen: a native promise operand costs exactly one job before
  the request settles, and the call itself runs no job.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  [...]
  7. Let promiseCompletion be Completion(PromiseResolve(%Promise%, completion.[[Value]])).
  [...]
  13. Perform PerformPromiseThen(promise, onFulfilled, onRejected).

  PromiseResolve returns a promise whose constructor is %Promise% unchanged, so
  no NewPromiseResolveThenableJob is queued for it.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

async function* g() {}

function ticks(log, n) {
  var p = Promise.resolve();
  for (var i = 1; i <= n; i++) {
    (function (i) { p = p.then(function () { log.push('w' + i); }); })(i);
  }
  return p;
}

asyncTest(async function () {
  var log = [];
  Promise.resolve().then(function () { log.push('job'); });
  var result = g().return(Promise.reject('E'));
  log.push('sync-end');
  var settled = result.then(function () { log.push('fulfilled'); }, function () { log.push('rejected'); });
  await Promise.all([settled, ticks(log, 4)]);
  assert.compareArray(
    log,
    ['sync-end', 'job', 'w1', 'rejected', 'w2', 'w3', 'w4'],
    'a rejected native promise operand rejects the request one job after the call'
  );

  log = [];
  result = g().return(Promise.resolve('V'));
  log.push('sync-end');
  settled = result.then(function (r) { log.push('fulfilled:' + r.value); });
  await Promise.all([settled, ticks(log, 3)]);
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'fulfilled:V', 'w2', 'w3'],
    'a fulfilled native promise operand settles the request one job after the call'
  );

  log = [];
  result = g().return({ then: function (resolve) { resolve('T'); } });
  log.push('sync-end');
  settled = result.then(function (r) { log.push('fulfilled:' + r.value); });
  await Promise.all([settled, ticks(log, 4)]);
  assert.compareArray(
    log,
    ['sync-end', 'w1', 'w2', 'fulfilled:T', 'w3', 'w4'],
    'a thenable operand costs the NewPromiseResolveThenableJob plus the reaction'
  );
});
```

### `test262-extra/async-generator-await-return-pending-gc-rooting.js`

```js
// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  While AsyncGeneratorAwaitReturn waits for its operand, the generator, the
  request being served and the requests queued behind it stay reachable across
  a garbage collection even though nothing else references them.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  The fulfilledClosure and rejectedClosure capture generator and complete its
  first queued request, then drain the rest of the queue, so all of it must
  stay live until the operand settles.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, host-gc-required]
---*/

function start(settle) {
  return new Promise(function (done) {
    var release;
    var operand = new Promise(function (resolve, reject) {
      release = settle === 'reject' ? reject : resolve;
    });
    var log = [];
    (function () {
      var it = (async function* () {})();
      it.return(operand).then(
        function (r) { log.push('return:' + r.value + ':' + r.done); },
        function (e) { log.push('return-rejected:' + e); }
      );
      it.next().then(function (r) { log.push('next:' + r.done); });
      it.throw('T').then(function () {}, function (e) { log.push('throw:' + e); });
    })();
    operand = null;
    setTimeout(function () {
      $262.gc();
      setTimeout(function () {
        $262.gc();
        release('V');
        setTimeout(function () { done(log); }, 0);
      }, 0);
    }, 0);
  });
}

asyncTest(async function () {
  assert.compareArray(
    await start('fulfill'),
    ['return:V:true', 'next:true', 'throw:T'],
    'fulfilled operand: every queued request settles in order after gc'
  );
  assert.compareArray(
    await start('reject'),
    ['return-rejected:V', 'next:true', 'throw:T'],
    'rejected operand: every queued request settles in order after gc'
  );
});
```

### `test262-extra/async-generator-requests-do-not-drain-microtasks.js`

```js
// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorcompletestep
description: >
  Settling an async generator request only enqueues the reactions of its
  promise. A job queued before next/return/throw was called does not run inside
  the call, whichever path settles the request.
info: |
  AsyncGeneratorCompleteStep ( generator, completion, done [ , realm ] )

  [...]
  8. Perform ! Call(promiseCapability.[[Resolve]], undefined, « iteratorResult »).
  9. Return unused.

  Calling a promise capability's resolve or reject function enqueues jobs; it
  never runs them. The request methods return the promise to their caller
  before any job runs.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, async-explicit-resource-management]
---*/

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 12; i++) p = p.then(function () {});
  return p;
}
function boom(v) { return function () { throw v; }; }
var badIterable = {};
badIterable[Symbol.iterator] = boom('iterable');
var laterBad = {};
laterBad[Symbol.iterator] = function () {
  var n = 0;
  return { next: function () { if (n++) throw 'next'; return { value: 1, done: false }; } };
};
function delegate(methods) {
  var it = { next: methods.next || function () { return { value: 1, done: false }; } };
  if (methods.return) it.return = methods.return;
  it[Symbol.asyncIterator] = function () { return this; };
  return it;
}
function first(it) { return it.next(); }

var scenarios = {
  'next: body throws': [async function* () { throw 'E'; }, null, function (it) { return it.next(); }],
  'next: completed': [async function* () {}, first, function (it) { return it.next(); }],
  'next: resumes into a throw': [async function* () { yield 1; throw 'E'; }, first, function (it) { return it.next(); }],
  'next: try/finally throw': [async function* () { try { yield 1; throw 'E'; } finally {} }, first, function (it) { return it.next(); }],
  'next: try/catch rethrow': [async function* () { try { yield 1; throw 'E'; } catch (e) { throw 'R'; } }, first, function (it) { return it.next(); }],
  'next: while condition throws': [async function* () { var i = 0; while (i++ < 1 || boom('E')()) { yield 1; } }, first, function (it) { return it.next(); }],
  'next: switch discriminant throws': [async function* () { switch (boom('E')()) { case 1: yield 1; } }, null, function (it) { return it.next(); }],
  'next: switch case test throws': [async function* () { switch (1) { case boom('E')(): yield 1; } }, null, function (it) { return it.next(); }],
  'next: for-of iterable throws': [async function* () { for (var x of badIterable) { yield x; } }, null, function (it) { return it.next(); }],
  'next: for-of step throws': [async function* () { for (var x of laterBad) { yield x; } }, first, function (it) { return it.next(); }],
  'next: yield* next() throws': [async function* () { yield* delegate({ next: boom('E') }); }, null, function (it) { return it.next(); }],
  'next: yield* result is not an object': [async function* () { yield* delegate({ next: function () { return 1; } }); }, null, function (it) { return it.next(); }],
  'return: suspended-start': [async function* () {}, null, function (it) { return it.return(1); }],
  'return: suspended-start, rejected promise': [async function* () {}, null, function (it) { return it.return(Promise.reject('E')); }],
  'return: suspended-start, thenable': [async function* () {}, null, function (it) { return it.return({ then: function (r) { r(1); } }); }],
  'return: completed': [async function* () {}, first, function (it) { return it.return(1); }],
  'return: suspended-yield': [async function* () { yield 1; }, first, function (it) { return it.return(1); }],
  'return: suspended-yield in try/finally': [async function* () { try { yield 1; } finally {} }, first, function (it) { return it.return(1); }],
  'return: await using disposer throws': [async function* () { await using d = { async [Symbol.asyncDispose]() { throw 'D'; } }; yield 1; }, first, function (it) { return it.return(1); }],
  'return: yield* return() throws': [async function* () { yield* delegate({ return: boom('E') }); }, first, function (it) { return it.return(1); }],
  'throw: suspended-yield': [async function* () { yield 1; }, first, function (it) { return it.throw('E'); }],
  'throw: completed': [async function* () {}, first, function (it) { return it.throw('E'); }],
  'throw: await using disposer throws': [async function* () { await using d = { async [Symbol.asyncDispose]() { throw 'D'; } }; yield 1; }, first, function (it) { return it.throw('T'); }],
  'throw: yield* has no throw()': [async function* () { yield* delegate({}); }, first, function (it) { return it.throw('T'); }]
};

asyncTest(async function () {
  for (var name in scenarios) {
    var it = scenarios[name][0]();
    if (scenarios[name][1]) await scenarios[name][1](it);
    await flush();

    var log = [];
    Promise.resolve().then(function () { log.push('job'); });
    var request = scenarios[name][2](it);
    log.push('returned');
    request.then(function () {}, function () {});
    await flush();
    assert.compareArray(log, ['returned', 'job'], name);
  }
});
```
