# Plan: issue #679 — JetStream async-fs: File's DataView field reads back undefined mid-run

## 1. Problem restated

`generators/async-file-system.js` consumes an `async function*` with `for await`.
Under allocation pressure the loop variable (and, in a manual-`next()` probe, the
whole iterator-result object) arrives as `undefined` / a hollow object, so
`new File(fileContents)` stores `undefined` and a later `this._data.byteLength`
throws `TypeError`, which nothing observes.

**Root cause (confirmed by experiment, not suspected):** the scheduler's
`async_gen_queues` — each entry an `AsyncGenRequest { kind, value, promise,
resolve_fn, reject_fn }` — is **not a GC root**. `JobScheduler::for_each_root`
(`src/interpreter/scheduler.rs:223`) visits `microtask_queue` and `timers` only;
`collect_gc_roots` (`src/interpreter/gc.rs:443`) delegates to it, and
`gc.rs` never mentions `async_gen_queue*`. `AsyncGeneratorEnqueue` appends the
request to the queue and `async_gen_process_queue` (`eval/generator_runtime.rs`
~2552) runs the generator body while the request is still parked there (it is
popped only after the step settles). The request's `promise` and its resolving
functions are, at that moment, held only by Rust locals and the queue — neither
traced. A major collection at any safepoint inside the generator body (here the
`for (let i…) view[i] = …` back-edge in `randomFileContents`) frees the promise
`P` that `next()` is about to return, plus its `resolve_fn`/`reject_fn`. The
arena recycles those ids; `next()` then hands the consumer an id that now names an
unrelated object (empty own keys, `value`/`done` absent → `undefined`), and the
`for await` head silently binds `undefined` (`iterator_complete` on a non-`done`
object is `false`, `iterator_value` defaults to `undefined`). No exception is
raised at the corruption site, which is why it surfaced only as a late
`TypeError`.

### Evidence (all reproduced in this planning stage, release build of HEAD `1b4c3fa8`, scratch binary under `$TMPDIR`)

- The issue's exact single-iteration repro **passes** on default HEAD (unrelated
  fixes since 0.8.2), but still fails on `--bytecode`
  (`REJ Cannot read properties of undefined (reading 'byteLength')`, 3/3), and on
  the default engine when `runIteration(i)` is driven for `i` in `0..6` (fails at
  `i == 2`). `--bytecode` does not host the bug (no `await`/`yield` compilation);
  it only raises allocation rate per iteration.
- `fileContents === undefined` is `true` and `typeof` is `"undefined"` — a genuine
  `undefined`, not a dangling object id. Replacing the `for await` head with a
  manual `const r = await gen.next()` shows `r` is an object with **empty
  `Object.keys(r)`** (`BADR n=96 r=object keys= valtype=undefined done=undefined`)
  — the fault is in the async generator's `next()` delivery, upstream of `File`
  and of the `for await` head.
- Scratch env-gated switch in `GcPacer::begin_collection` (not kept):
  `JSSE_NOGC=1` → passes; **`JSSE_NOGC=major` → passes; `JSSE_NOGC=minor` → still
  fails.** A *major* collection is the trigger. (Why the minor collection does not
  reclaim it — plausibly `object_requires_persistent_minor_scan` keeping
  `ObjectKind::Iterator` objects scanned — is **unverified**.)
- Scratch `WATCH-FREED` instrumentation (not kept) on the iterator-result objects
  created by the async-generator yield microtask, with
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, captured this chain at the free:
  `gc_collect_major ← gc_safepoint ← exec_prepared_statements ←
  exec_statements_cached ← exec_body_inner ← exec_state_machine_body ←
  async_generator_next_state_machine_impl ← async_generator_next_state_machine_with_promise
  ← async_gen_process_queue ← async_gen_enqueue ← async_generator_next ←
  call_function ← eval_call ← async_function_resume ← drain_microtasks`. I.e. the
  collection fires while the generator body runs under the queue driver, invoked
  from the consumer's `next()` call — exactly when the request sits in the
  untraced queue. (The watched object itself freed at age 1 may be ordinary
  garbage; the chain, not that id, is the evidence.)
- **Fix confirmed:** adding the four request fields to `for_each_root` (patch
  below) → `--bytecode` manual-`next()` probe: 0 `BADR`/`REJ`; `--bytecode` issue
  repro prints `C`,`D`; default-engine 6-iteration driver prints `D5`.
- Ruled out (earlier draft of this plan): `async_gen_yield_pending` being an
  unkeyed `bool` (single live async generator during `setupDirectory`, so no
  cross-generator clobbering); `eval.rs` `ForOfHead`/`async_fn_suspend_at_await`;
  an inline-cache or bytecode-VM defect; a write-barrier gap on promise state
  (`fulfill_promise` goes through `borrow_mut`, which runs the barrier).
- Separately observed at HEAD: minimal deterministic cases (§4) also fail:
  `g().next()` with `$262.gc()` in `g`'s body before the first `yield` throws
  `TypeError: undefined is not a function` synchronously (`p.then` is looked up
  on a recycled id); and a generator with three queued requests carrying object
  send-values, with `$262.gc()` between two resumptions, never settles the final
  promise (silent, exit 0) — the `AsyncGenRequest.value` is unrooted too.

### The scratch fix (validated; the implementation stage re-applies it)

```diff
--- a/src/interpreter/scheduler.rs
+++ b/src/interpreter/scheduler.rs
@@ pub(crate) fn for_each_root(&self, mut visit: impl FnMut(&JsValue)) {
         for (roots, _) in &self.microtask_queue {
             for value in roots {
                 visit(value);
             }
         }
+        for queue in self.async_gen_queues.values() {
+            for request in queue {
+                visit(&request.value);
+                visit(&request.promise);
+                visit(&request.resolve_fn);
+                visit(&request.reject_fn);
+            }
+        }
         for (callback, args) in self.timers.iter_roots() {
```

`collect_gc_roots` already funnels both `gc_collect_minor` (gc.rs:585) and
`gc_collect_major` (gc.rs:756) through `for_each_root`, so one edit covers both.

## 2. Spec basis

Reclaiming a reachable object is unobservable in the spec — there is no clause
under which GC timing may change what a program reads. The engine bug violates
the following clauses by making their observable results depend on GC. Anchors
are `spec/spec.html` ids (all under ECMA-262 §27.6 AsyncGenerator Objects;
`AsyncGeneratorYield` is §27.6.3.8 as the engine's own comments cite it):

- **AsyncGeneratorRequest Records** (`#sec-asyncgeneratorrequest-records`) and
  **AsyncGeneratorEnqueue** (`#sec-asyncgeneratorenqueue`): the request — its
  `[[Completion]]` (the sent value) and `[[Capability]]` (promise + resolving
  functions) — lives in `[[AsyncGeneratorQueue]]` until the generator settles it.
  The queue owns the capability; the engine's queue must therefore keep it alive.
- **%AsyncGeneratorPrototype%.next** (`#sec-asyncgenerator-prototype-next`):
  returns `promiseCapability.[[Promise]]` — the very promise the request settles.
- **AsyncGeneratorCompleteStep** (`#sec-asyncgeneratorcompletestep`),
  **AsyncGeneratorYield** (`#sec-asyncgeneratoryield`) and
  **AsyncGeneratorDrainQueue** (`#sec-asyncgeneratordrainqueue`): each yield
  resolves *that request's* capability with an iterator result carrying the
  yielded value.
- **ForIn/OfBodyEvaluation**
  (`#sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`):
  for async iteration each turn binds `IteratorValue(? Await(IteratorNext(...)))` —
  that turn's own result, never an unrelated object.
- **OrdinaryGet** (`#sec-ordinaryget`): an unmodified own data property reads back
  the last stored value (the reason `file._data` must not change).

## 3. Files to touch

Engine:

- `src/interpreter/scheduler.rs` — `JobScheduler::for_each_root` (~223): visit
  every `AsyncGenRequest` field (patch above). Add a unit test in its existing
  `#[cfg(test)]` module next to `async_gen_queues_are_isolated_per_generator`
  (~431).
- `src/interpreter/gc.rs` — `free_gc_object` (~725), which already drops the
  id-keyed side tables (`iterator_next_cache`, `generator_inline_iters`,
  `generator_for_of_stacks`) *because the arena recycles ids*: drop
  `async_gen_queues[id]` there too (slice 3). Needs a small `pub(crate)
  remove_async_gen_queue(gen_id)` accessor on `JobScheduler` (no accessor removes
  a queue today; `scheduler.rs` only has `entry`/`get`/`get_mut`).

Tests (new files):

- `test262-extra/AsyncGenerator-next-request-promise-gc-rooting.js` (slice 1).
- `test262-extra/AsyncGenerator-queued-request-value-gc-rooting.js` (slice 2).

`test262-extra/` is flat (no `language/` subtree); the precedent names to follow
are `Array-length-set-gc-rooting.js`, `Promise-allKeyed-combinator-gc-rooting.js`
and `agent-get-report-async-promise-gc-rooting.js` (same header shape:
`esid`, `description`, `info` naming the clause and the issue, `flags: [async]`,
`features: [host-gc-required]`, `$262.gc()`).

Non-engine: none. No `docs/adr/` (no new architecture). If the tree's `CONTEXT.md`
has a "Rooted Slot"/GC-root vocabulary section, one sentence noting that
"scheduler-owned request queues are roots" is optional, not required.

## 4. TDD slices

Build with a capped parallelism and a scratch target dir, e.g.
`CARGO_TARGET_DIR=$TMPDIR/target cargo build --release -j8`. Fresh workspaces
have empty `test262/` and `spec/` submodules — `git submodule update --init
--depth 1 test262` before running the suite.

1. **Red, then green: request promise survives a collection inside the body.**
   Add `test262-extra/AsyncGenerator-next-request-promise-gc-rooting.js`
   (`flags: [async]`, `features: [host-gc-required]`, `esid:
   sec-asyncgeneratorenqueue`). Body:
   ```js
   async function* g() {
     $262.gc();
     var junk = [];
     for (var i = 0; i < 64; i++) junk.push({ i: i, s: "x" + i });
     yield 42;
   }
   g().next().then(function (r) {
     assert.sameValue(r.value, 42);
     assert.sameValue(r.done, false);
   }).then($DONE, $DONE);
   ```
   The `$262.gc()` runs while the request is at the front of the queue and the
   promise has not yet been returned; the churn forces id reuse. **Verified red at
   HEAD** (`TypeError: undefined is not a function`, thrown synchronously from
   `.then`), identically with `--bytecode`. Green with the `for_each_root` patch.
   Lead with this slice: it is the clean deterministic red.
2. **Queued request values and capabilities.** Add
   `test262-extra/AsyncGenerator-queued-request-value-gc-rooting.js`. A generator
   `h` does `var a = yield 1; $262.gc(); churn; var b = yield 2;` and records
   `a.tag`, `b.tag`; the driver issues `it.next()`, `it.next({tag:"A"})`,
   `it.next({tag:"B"})` (object send-values held *only* by the queue) and a final
   `it.next().then(...)`. Assert `log` equals `["A","B"]` **via `$DONE`** — on
   HEAD the final promise never settles (verified: no output, exit 0), so a test
   that merely "does not throw" would pass on broken HEAD; `flags: [async]` makes a
   never-settling promise a timeout failure. Because red is a timeout, run it with
   the runner's `--timeout` lowered (e.g. `uv run python scripts/run-test262.py
   --timeout 15 test262-extra/AsyncGenerator-queued-request-value-gc-rooting.js`)
   rather than waiting the 120 s default. Green with the same patch.
3. **Unit test + queue cleanup on free.** In `scheduler.rs` tests add
   `for_each_root_visits_every_async_gen_request_field` (four distinct object
   values in one `AsyncGenRequest`; assert all four are visited; assert an empty
   queue visits nothing). Then, red→green, cover the leak the fix introduces:
   `async_gen_queues` entries are **never removed today** (only `entry`/`get`/
   `get_mut` exist), so rooting them would make an abandoned generator's last
   queued request — a promise plus two closures — immortal, *and* make each
   collection walk every generator the program ever enqueued on (JetStream's
   async-fs creates thousands of short-lived `ls()`/`forEach*` generators). A stale
   entry is also a correctness hazard independent of rooting: the arena recycles
   ids, so a new async generator allocated in a freed generator's slot inherits its
   leftover requests. Fix: `self.scheduler.remove_async_gen_queue(id)` in
   `free_gc_object`, with a `gc.rs` unit test (pattern: the tests around
   `gc.rs:1568`/`1826` — build an interpreter, enqueue a request for a generator
   object, drop the last reference, `gc_collect_major`, assert the queue and its
   promise are gone). If this slice turns out to exceed ~30 lines or perturbs
   `test262-pass.txt`, drop it from the PR and file it as a follow-up issue — the
   rooting fix in slice 1–2 stands alone.
4. **End-to-end acceptance (manual, not committed).** Regenerate the issue's
   driver (`scripts/run-jetstream.py`'s `build_polyfill_preamble` +
   `/tmp/JetStream/generators/async-file-system.js` at JetStream `c603c04`; clone
   it under `$TMPDIR` if absent) and confirm `C`,`D` for: default engine ×6
   `runIteration` (must print `D5`), and `--bytecode` ×1. These two — not the
   issue's single-iteration default-engine command, which already passes at HEAD —
   are the acceptance criteria. Report them in the PR body.

## 5. Test surface

- Targeted test262 directories (async-generator queue machinery):
  `test262/test/language/statements/for-await-of/`,
  `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/`,
  `test262/test/built-ins/AsyncGeneratorFunction/`,
  `test262/test/built-ins/AsyncGeneratorPrototype/`,
  `test262/test/built-ins/AsyncFromSyncIteratorPrototype/`,
  `test262/test/built-ins/AsyncIteratorPrototype/`. None single-handedly catches
  this bug (each drives one generator and never forces a collection mid-step);
  they are the regression net for the queue driver.
- GC-sensitive suites, since roots changed: `test262/test/built-ins/WeakRef/`,
  `test262/test/built-ins/FinalizationRegistry/`, `test262/test/built-ins/Promise/`.
- New `test262-extra/` files (slices 1–2), run with
  `uv run python scripts/run-test262.py test262-extra/` (no dedicated runner).
  These belong in `test262-extra/`, not `tests/`, because the failure changes an
  observable ECMAScript value (which object `next()` returns / what a
  `for await` variable binds), per this repo's rule; they cite
  `AsyncGeneratorEnqueue` and `AsyncGeneratorYield`.
- `cargo test --release` (slice 3 unit tests; also the existing scheduler test
  `async_gen_yield_pending_round_trip` must stay green — the flag is untouched).
- `uv run python scripts/run-custom-tests.py` for `tests/`.
- Full `uv run python scripts/run-test262.py` before opening the PR, compared
  against `origin/main:test262-pass.txt`. Do **not** pass `--update-baseline`.

## 6. Regression risk

- **Only ever retains more, never less.** Adding roots is correctness-safe; the
  risks are memory/time. Rooted values are bounded by *live queued requests*, but
  the map itself is never pruned (see slice 3): without the `free_gc_object`
  cleanup, `for_each_root` is O(number of async generators ever enqueued on) per
  collection and abandoned-generator requests are immortal. That is why slice 3 is
  recommended in the same PR; canaries are the long Node-compat harnesses
  (`big.js`, `uglify-js`, `highlight.js`) and the JetStream async-fs driver, where
  a regression shows as slowdown/timeout rather than a wrong answer.
- **Cleanup-on-free hazard (slice 3):** dropping a queue is only sound if the
  generator is truly unreachable — it is, because `free_gc_object` runs only on
  unmarked objects, and the request roots do not mark the generator. Verify the
  generator being *stepped* (`this` in `async_gen_enqueue`, a Rust local) survives
  the same collection; slice 1 already exercises that (the body calls
  `$262.gc()` mid-step) and passes with the scratch fix.
- **Not addressed, same family (follow-ups, do not bundle):** the native
  closures `asyncGenYieldFulfill`/`asyncGenYieldReject` and the microtask closures
  in `generator_runtime.rs` capture `gen_this`, `resolve_fn_c`, `reject_fn_c` as
  Rust captures invisible to the tracer (`gc.rs`'s `pin_native_root` doc describes
  exactly this hazard). The queue roots make `resolve_fn`/`reject_fn` safe *while
  the request is queued*, which is the whole window today, but `gen_this` is only
  as safe as the caller's reference. Audit them in a separate issue.
- **Baseline:** expected neutral (no spec-behavior change). A newly passing
  test262 test would be surprising; cross-check against spec before accepting it.
- **Bytecode fast path:** untouched (`bytecode/vm.rs` has no suspend/resume);
  `--bytecode` merely surfaces the bug sooner.

## 7. Out of scope

- The dropped-unhandled-rejection / exit-0-with-no-stderr behavior named in the
  issue: a host decision (HostPromiseRejectionTracker), separate from this bug.
  #681 already closed the runner-side "no JSON output" misclassification.
- Hardening `iterator_complete`/`iterator_value` to reject a non-`IteratorResult`
  (would have turned this silent corruption into an immediate `TypeError`, but
  treats the symptom).
- `async_gen_yield_pending` (single `bool` on `JobScheduler`) — ruled out here; do
  not re-scope it under #679.
- `eval.rs` `ForOfHead` / `async_fn_suspend_at_await`, and any redesign of the
  AsyncGenerator queue/drain protocol.
- The `gen_this`/native-closure capture audit noted in §6.
- Updating the issue's stale repro command belongs in an issue comment, not the
  PR diff.
- Any `scripts/` or JetStream-harness change.
