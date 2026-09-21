# Plan: issue #707 — `for await` in an async-generator body drains the microtask queue inline

## 1. Problem restated

`async_generator_next_state_machine_impl` (`src/interpreter/eval/generator_runtime.rs`)
drives a `for await` loop's per-iteration `Await(nextResult)` step (the
`StateTerminator::ForOfHead` arm, `is_await` branch) by calling the blocking
`self.await_value(&step_result)`, which synchronously drains the microtask
queue until that one promise settles before returning control to the caller.
This makes `it.next()` on an async generator with a `for await` in its body
resolve foreign, unrelated microtasks *inline*, out of order, instead of
suspending the generator and resuming it from a genuine `PerformPromiseThen`
job like every other `Await` in the engine does. `async_function_resume`
(`src/interpreter/eval.rs`) already drives the equivalent `for await` site in
plain async functions correctly by suspending; this plan ports that same
suspension to the async-generator driver, and (as a direct, unavoidable
consequence of moving from a synchronous to a suspended completion path) also
ports the associated protocol-failure short-circuit that a rejected head
`Await` requires, so the fix does not trade one spec violation for another.

## 2. Spec basis

- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`**
  ("ForIn/OfBodyEvaluation"), `spec/spec.html:22388-22465`. The `Repeat` loop's
  step 2 (`spec/spec.html:22412`): "If `iteratorKind` is `~async~`, set
  `nextResult` to `? Await(nextResult)`." This is the step the buggy code
  implements with a blocking call; the fix must implement it as a genuine
  suspension.
- **`await`** (the `Await` abstract operation), `spec/spec.html:51047-51081`.
  Steps 5–9: register `onFulfilled`/`onRejected` closures via
  `PerformPromiseThen`, then **remove the async context from the execution
  context stack** and return control to the caller — i.e. suspend, don't
  block. This is the mechanism `async_function_resume` already implements
  (`async_fn_suspend_at_await`, `src/interpreter/eval.rs:9715`) and the one
  the state-machine async-generator driver already implements for its own
  `StateTerminator::Await` arm (`src/interpreter/eval/generator_runtime.rs:5945-6042`)
  — just not yet for `ForOfHead`.
- **Same ForIn/OfBodyEvaluation clause, IteratorClose placement**
  (`spec/spec.html:22445-22462`): `AsyncIteratorClose`/`IteratorClose` is only
  called from two places — after the binding/assignment `status` is abrupt
  (step at `22445`), and after the loop body's `result` is abrupt (step at
  `22455`). The raw, `?`-prefixed retrieval steps that precede them —
  `? Call(nextMethod, ...)`, `? Await(nextResult)`, the not-an-Object check,
  `? IteratorComplete`, `? IteratorValue` (`22410-22416`) — are **not**
  wrapped in such a call; an abrupt completion from any of them propagates
  out of `ForIn/OfBodyEvaluation` directly, with no `return()` call on that
  iterator. This is why suspending at `Await(nextResult)` cannot simply route
  a later rejection through the driver's generic exception-routing path:
  that path *does* call `IteratorClose` for every still-open loop it
  unwinds, which is correct for loops crossed by a body/binding abrupt
  completion but wrong for the loop whose own `Await(nextResult)` just
  rejected.

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the only production file.
  - Add a new private method (name: `async_gen_suspend_at_await`, placed
    directly after `async_generator_next_state_machine_impl`, near
    `async_gen_await_resume`) that extracts the suspend mechanics already
    inlined in the `StateTerminator::Await` arm (`5962-6042`: `promise_resolve_value`,
    set the generator's `IteratorState` to `StateMachineAsyncGenerator` with
    `execution_state: SuspendedAtState`, build the `asyncGenAwaitFulfill`/
    `asyncGenAwaitReject` native closures, `promise_then`,
    `set_async_gen_yield_pending(true)`), parameterized by `resume_state`,
    `sent_value_binding`, `pending_return`, and the raw `await_val`. The
    `StateTerminator::Await` arm is rewritten to call this method instead of
    inlining it (behavior-preserving refactor — same fields, same order).
  - Rewrite the `StateTerminator::ForOfHead` arm's `is_await` branch
    (currently `5716-5750`) to mirror `async_function_resume`'s `ForOfHead`
    handling (`src/interpreter/eval.rs:9385-9440`): check for a cached
    `"{iter_var}__await"` temp binding first (a resumed call); on a cache
    miss (first entry to this state), call `iterator_next` once, declare the
    temp (`BindingKind::Var`) if absent, and suspend via
    `async_gen_suspend_at_await` with `resume_state` set to *this same*
    `ForOfHead` state id, `sync_generator_for_of_stack(o.id, &for_of_stack)`
    called first (the preceding `iteration_env.take()` dispose step mutates
    the local `for_of_stack`; that mutation must be persisted to
    `self.generator_for_of_stacks` before the driver returns, or a resumed
    call reloads a stale `iteration_env` and disposes it twice), and
    `self.in_state_machine = saved_in_state_machine;` restored immediately
    after the call before returning — matching `async_function_resume`'s own
    `ForOfHead` suspend site (`eval.rs:9438`), not the (separately
    out-of-scope) `StateTerminator::Await` arm, which does not restore this
    flag today.
  - Add the protocol-failure short-circuit for a *rejected* head `Await`:
    - Before the existing "apply `pending_binding` to `sent_value`" block at
      `3964` consumes it, stash a clone (`pending_binding.clone()`) for later
      inspection — the check below needs `for_of_stack`, which is not
      restored from `self.generator_for_of_stacks` until `4014-4018`, after
      that block already moves `pending_binding`.
    - After `for_of_stack` is restored, compute (mirroring
      `async_function_resume`'s `for_of_protocol_failure`,
      `src/interpreter/eval.rs:8219-8233`): if there is a pending exception
      *and* the stashed binding is `Variable(name)` where `name` ends in
      `"__await"` *and* `for_of_stack` has a loop whose `iter_var` matches
      the stripped name, record that `iter_var` as the failed loop.
    - In the `check_abrupt_on_resume` block's pending-exception handling
      (`4099-4103`), immediately before `route_exception!(exc)` is called:
      if a protocol failure was recorded, remove that loop's entry from
      `for_of_stack` directly (`unroot_for_of_iterator`, then
      `sync_generator_for_of_stack`) instead of letting the generic
      `route_exception!` → `route_generator_exception` →
      `unwind_generator_for_of_loops` → `close_for_of_loop` path run
      `IteratorClose` on it. Other, still-open outer loops the exception
      crosses are unaffected and continue to close normally.
    - Scope note: `async_function_resume` sets `for_of_protocol_failure` at
      five sites (iterator_next error, not-an-object, `IteratorComplete`
      error, `IteratorValue` error, and the rejected `Await`). Only the
      rejected-`Await` case crosses a suspension boundary in the
      async-generator driver; the other four are already handled
      synchronously and correctly today by the existing
      `discard_failed_generator_for_of_loop` calls in this same arm
      (`5695`, `5762`/nearby `Err` arms, `5891`). Only the `Await` case needs
      the new resume-time flag — do not add the other four.
- No `docs/adr/` or `CONTEXT.md` changes: this reuses the existing
  `StateMachineAsyncGenerator` / `SuspendedAtState` / `PerformPromiseThen`
  suspend vocabulary already documented from `#703`/`#715`'s work, applied to
  one more call site. It introduces no new architectural concept.

## 4. TDD slices

1. **Core fix — suspend the `for await` head instead of blocking.**
   - Red: add `test262-extra/async-generator-for-await-suspends-microtask-queue.js`,
     a direct port of the issue's repro (`for await (var y of [1])` inside an
     `async function*`, interleaved with a pre-scheduled `Promise.resolve().then()`
     chain (`w1`…`w5`), asserting via `assert.compareArray` (checked from a
     `setTimeout`, `flags: [async]`, following the
     `async-generator-switch-case-test-yield-await-is-a-suspension-point.js`
     style already in the tree) that the log is
     `['body', 'after-next', 'w1', 'w2', 'y1', 'w3', 'w4', 'w5']` — the order
     the issue records from Node and derives from `Await` suspending via a
     job, not from draining inline. Confirm against current `main` that the
     test fails with today's `['body', 'w1', 'w2', 'y1', 'w3', 'w4', 'after-next', 'w5']`
     before implementing.
   - Green: implement the `async_gen_suspend_at_await` extraction and the
     `ForOfHead` rewrite described in §3.
   - Also add `test262-extra/async-generator-for-await-native-iterator-tick-order.js`,
     modeled on `test262/test/language/statements/for-await-of/ticks-with-async-iter-resolved-promise-and-constructor-lookup.js`
     but for an `async function*` body: a hand-written `[Symbol.asyncIterator]`
     object (no array — `CreateAsyncFromSyncIterator` adds its own extra tick,
     which would blur what this test is isolating) whose `next()` returns an
     already-resolved promise. Derive the expected tick log by counting one
     `Await` per loop pass — including the terminal pass that observes
     `done: true`, since `? Await(nextResult)` in `ForIn/OfBodyEvaluation` is
     unconditional — rather than by copying a number from Node; use `node`
     only to cross-check the derivation, never as the source of truth.
2. **Protocol-failure carve-out for a rejected head `Await`.**
   - Red: add `test262-extra/async-generator-for-await-reject-does-not-close-iterator.js`:
     a custom async iterable whose `next()` returns a rejected promise and
     whose `return()` sets a flag, used in a `for await` wrapped in a
     `try`/`catch` inside an `async function*`; assert the `catch` receives
     the rejection reason, the generator continues (a subsequent `yield`/`next()`
     still works), and the `return()` flag is never set. Written against the
     slice-1 code, this fails because the generic exception-routing path
     calls `close_for_of_loop` (→ `IteratorClose`) on the loop whose own
     `Await(nextResult)` rejected.
   - Green: implement the `pending_binding` stash/recompute and the
     `check_abrupt_on_resume` short-circuit described in §3.

## 5. Test surface

- Targeted test262 regression runs (must not newly fail):
  - `uv run python scripts/run-test262.py test262/test/language/statements/for-await-of/`
    (1234 tests; includes the async-function `ticks-with-async-iter-resolved-promise-and-constructor-lookup*.js`
    tests this plan's slice-1 test is modeled on, and the `async-gen-decl-dstr-*-iter-*-close*.js`
    destructuring-iterator-close tests, which exercise `ForOfHead` in async
    generators today and must keep passing unchanged).
  - `uv run python scripts/run-test262.py test262/test/language/statements/async-generator/`
  - `uv run python scripts/run-test262.py test262/test/language/expressions/async-generator/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/AsyncGeneratorFunction/`
  - `uv run python scripts/run-test262.py test262/test/built-ins/AsyncGeneratorPrototype/`
- New test262-extra tests (spec-correct behavior test262 does not cover for
  the async-generator case, though it covers the async-function analogue):
  `test262-extra/async-generator-for-await-suspends-microtask-queue.js`,
  `test262-extra/async-generator-for-await-native-iterator-tick-order.js`,
  `test262-extra/async-generator-for-await-reject-does-not-close-iterator.js`.
  Run via `uv run python scripts/run-test262.py test262-extra/`.
- `cargo test --release` for the Rust-level regression suite (the file
  `src/interpreter/tests.rs` already has async-generator-suspension unit
  tests from `#715`'s work; no new Rust unit test is planned since the
  observable behavior is fully covered by the test262-extra files above and
  the engine has no lower-level seam to unit-test this at).
- Full `uv run python scripts/run-test262.py` before opening the PR, compared
  against the `origin/main:test262-pass.txt` baseline (read-only — do not
  pass `--update-baseline`).

## 6. Regression risk

- **`ForOfHead` in async generators generally**: this arm is reached by every
  `for await` in an async-generator body, including the widely-exercised
  destructuring-iterator-close test262 family
  (`async-gen-decl-dstr-array-elem-iter-nrml-close*.js` and siblings). The
  rewrite changes control flow (cache-check-then-suspend vs.
  call-then-block) but must leave every non-`is_await` and every
  already-passing `is_await` case byte-for-byte equivalent in outcome — the
  targeted directories in §5 are the guard against a baseline regression
  here.
- **`for_of_stack` / `self.generator_for_of_stacks` sync discipline**: the
  new suspend path returns mid-arm, after the arm's own
  `iteration_env.take()` mutation. Missing the `sync_generator_for_of_stack`
  call before returning is the specific failure mode to watch for — it would
  silently double-dispose an `await using`/`using` iteration resource on
  resume (a GC-rooting/use-after-dispose class of bug, not a panic, so it
  needs the targeted for-of tests, not just `cargo test`, to surface).
- **`check_abrupt_on_resume` / exception-routing ordering**: the protocol-failure
  short-circuit must run *before* `route_exception!` on every resume with a
  pending exception, not just the for-of-head one — an off-by-position
  insertion could either miss the carve-out (regressing to the
  `IteratorClose`-on-rejected-head-Await bug this plan calls out in §2) or
  over-apply it to unrelated pending exceptions (suppressing a legitimate
  `IteratorClose` on some other resumed `Await`). The slice-2 test in §4 is
  the direct guard; the broader for-await/for-of test262 directories in §5
  are the indirect one.
- **GC rooting of the new suspension**: the new call site reuses
  `async_gen_suspend_at_await`'s `StateMachineAsyncGenerator`/`SuspendedAtState`
  representation and closure-capture pattern verbatim from the already-proven
  `StateTerminator::Await` arm (hardened by `#706`'s GC-rooting fix and
  exercised continuously since). No new GC surface is introduced, so this is
  a low-risk area, but `cargo test --release` (which exercises
  `src/interpreter/gc.rs`'s mark-and-sweep over live generator state) remains
  part of the gate.
- **Bytecode fast path**: not implicated — `src/interpreter/bytecode/`
  contains no generator- or async-aware code; async generators are exclusively
  tree-walked through the state machine this plan edits.
- **Property MOP (`property.rs`)**: not implicated — no `[[Get]]`/`[[Set]]`/
  proxy/typed-array code path is touched.

## 7. Out of scope

- The other blocking `await_value` call sites already split out of `#687`
  into their own tracked issues — do not fold them into this PR:
  - `#708` — `yield*` delegated steps in async generators
    (`generator_runtime.rs:3343`, `3526`, `3740`).
  - `#709` — `await` in destructuring defaults.
  - `#710` — the `InlineYield` tree-walker fallback path (the
    `Completion::Yield` handling at `generator_runtime.rs:4419-4422`, which
    is the degraded-behavior backstop noted in `CLAUDE.md`'s architecture
    notes, not the state-machine `Await` terminator).
  - `#711` — deleting the dead legacy `IteratorState::AsyncGenerator` path
    (the `await_value` calls at `generator_runtime.rs:6503`, `6541` live
    there, not in the state-machine driver this plan touches).
- The `StateTerminator::Await` arm's own missing `self.in_state_machine =
  saved_in_state_machine` restore (§3) is a separate, narrower latent gap in
  existing code this plan does not touch, beyond extracting the shared
  `async_gen_suspend_at_await` helper it and the new `ForOfHead` site both
  call — the extraction is behavior-preserving for that arm by construction
  (the restore stays a caller-side concern, and only the new caller adds it).
  Worth a follow-up issue if it turns out to be observable; not this PR.
- The missing "`nextResult` is not an Object → `TypeError`" check
  (`spec/spec.html:22413`) on the async-generator `ForOfHead` `is_await`
  path (present in `async_function_resume`'s equivalent at `eval.rs:9455-9460`,
  absent here — `iterator_complete` silently treats a non-object result as
  `done: true` instead). This is a real, independent spec-compliance gap,
  but it is orthogonal to the inline-microtask-draining bug this issue
  tracks and is not required to fix it or to avoid regressing it; recommend
  filing it as its own `agent-ready` issue rather than bundling it here.
- No refactor of the `StateTerminator::ForOfHead` arm's non-`is_await`
  (plain `for-of`/`for-in`) branches, and no touching of the separate
  `iteration_env` disposal mechanism earlier in the same arm (`5645-5690`,
  `#665`/`#685`/`#686` territory) — both are left exactly as they are.
