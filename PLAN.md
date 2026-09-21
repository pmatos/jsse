# Plan: issue #710 — blocking `await_value` on the InlineYield fallback paths

## 0. Audit result (done in this planning stage — the issue's "prove unreachable" branch is closed)

Both sites are **reachable from ordinary JavaScript**. They are not dead code, so `unreachable!()`/deletion is off
the table; the sites must be converted. Evidence, from a release build with temporary hit-logging in the three
fallback sites (reverted; nothing of it is in the tree) and the full test262 run (99,911/99,911 passing with the
instrumentation in place):

- The fallback is entered when a state body's statement list returns `Completion::Yield` — i.e. when the transform
  (`generator_transform.rs::transform_yielding_expression`, default arm `emit_expression_with_binding`) leaves a
  `yield` inside an expression it does not decompose. Confirmed reachers, each verified in a sync and an async
  generator (`HIT`) or against `node` (behaviour):
  - destructuring **assignment** patterns with a suspension in a default/target: `({a = yield 1} = {})`,
    `[a = yield 1] = []`, `for ({a = yield} of …)`, `for await ([x = yield] of …)` — `extract_lhs_suspensions` has
    `_ => expr.clone()` so patterns are never hoisted (and hoisting a default out of a pattern would be wrong anyway:
    it is conditional and ordered against iterator stepping);
  - `x[yield 1] = yield 2` (the `Assign` arm hoists the RHS first and never looks at the LHS: wrong order too —
    jsse yields `2` then `1`, node `1` then `2`);
  - `for (a[yield 1] of …)`, `for (a[yield 1] in …)`, `super[yield 1]`.
- test262 exercises the fallback heavily for plain `yield` (hit counts from the instrumented `language/` run):
  **26 async-generator scenarios** (the 12 `language/statements/for-await-of/async-gen-decl-dstr-*-yield-expr.js`
  files × strict/sloppy = 24, plus 2 more) and **~96 sync scenarios** (`language/statements/for-of/dstr/*yield*`
  and similar dstr `yield` cases; exact file list not pinned — re-derive with the same hit-logging if needed). It
  exercises `yield*` in the fallback **zero** times (no `Expression::Yield` hit with `delegate=true`).
- Site 2 (`generator_runtime.rs`, `async_generator_next_state_machine_impl`, the `if let Completion::Yield(yield_val)
  = stmt_result` block: `self.await_value(&yield_val)` then `resolve_fn` + `drain_microtasks()`): witness chain
  `Promise.resolve().then(w1)…then(w6)` started before `it.next()` on `async function* g(){ var a; ({a = yield 1} =
  {}); }` logs `w1,w2,w3,w4,w5,w6,after-next,next-resolved`; a lowered `yield 1` and node both log
  `after-next,w1,w2,next-resolved,w3,w4,w5,w6`.
- Site 1 (`eval.rs`, `Expression::Yield` arm, `delegate` branch, `this.await_value(&next_result)`): reached only on a
  *replay* execution (`generator_context` is `Some` only when re-entering a state after an inline yield). On the
  *first* execution `generator_context` is `None`, so `is_async_gen` is `false` and the branch calls the **sync**
  `get_iterator`: `({a = yield* inner()} = {})` with an async-generator `inner` throws `TypeError: … is not
  iterable` (node yields 10, 20, then `"r"`). On replay (`async function* g(){ var a,b; ({a = yield 1, b = yield*
  [10]} = {}); }`, second `next("A")`) the blocking await runs: witness log `w1..w8,after-next,…` vs node
  `after-next,w1,w2,w3,next-resolved,…`.
- The fallback is unsound beyond blocking (recorded for the follow-ups, **not fixed here**): it re-executes the
  state's *preceding statements* and the yield operand on every resume — `var n=0,m=0; function f(){m++;return 1}
  function* g(){ n++; var a; [a = yield f()] = []; return [n,m,a] }` gives `[2,2,7]` on jsse, `[1,1,7]` on node.
  Also: `var {a = yield 1} = {}` / `let`/`const` and `catch ({a = yield 1})` swallow the yield and complete the
  generator immediately (`{"done":true}`; node yields 1 then returns 5) — a different, wrong-value bug.

**Measurement caveat.** Every witness log, the `TypeError`, and the hit counts above were taken on `cb19f1a8`, i.e.
*before* #723 rewrote ~1,100 lines of `generator_runtime.rs`. Both `await_value` sites and the inline
`Completion::Yield` block were verified still present on `origin/main` (`eval.rs` `yield*` loop; driver inline block),
so the results should reproduce, but re-run the RED tests on a **post-rebase, pre-change release build** before
trusting them.

## 1. Problem restated

Two spots on the degraded `generator_context` / `SentValueBindingKind::InlineYield` fallback of the state-machine
generator drivers call the blocking `Interpreter::await_value`, which runs a nested microtask loop inside the running
job and a request issued from a reaction then runs already-queued reactions before returning (#687's bug family):
(a) the async-generator `yield*` branch of `Expression::Yield` in `eval.rs`, and (b) the async driver's handling of an
inline `Completion::Yield` (`Await(value)` of the yielded operand followed by an immediate resolve + drain). Both are
reachable (§0), so they must become real suspension points (spec `Await` parks the generator and resumes in a later
job) rather than be deleted. Retiring the whole fallback (lowering destructuring patterns with suspensions in the
transform, #625) is the real fix for the class but is a separate, much larger project — see §7.

## 2. Spec basis

All in `spec/spec.html` (ecma262 submodule):
- `sec-generator-function-definitions-runtime-semantics-evaluation` — `YieldExpression : yield AssignmentExpression`
  (→ `Yield(value)`) and `yield * AssignmentExpression` (async: `Await(innerResult)` on every step, then
  `AsyncGeneratorYield(? IteratorValue(innerResult))` with **no** extra Await of the value; `done` → return
  `IteratorValue`).
- `sec-yield` — `Yield(value)`: async generator ⇒ `AsyncGeneratorYield(? Await(value))`.
- `sec-asyncgeneratoryield` — `AsyncGeneratorYield` (complete the front request, continue or suspend).
- `await` — `Await(value)`: `PromiseResolve` + `PerformPromiseThen`; the continuation is its own job (never runs
  other jobs inline). Also `sec-performpromisethen`.
- `sec-jobs` — a Job runs to completion only when the execution-context stack is empty (why a nested drain is wrong).
- `sec-runtime-semantics-destructuringassignmentevaluation` — evaluation order the destructuring-assignment cases in
  the tests depend on (target before iterator step; default initializer only when the value is `undefined`).

## 3. Files to touch

Baseline first: `git fetch origin main && git rebase origin/main` (the plan commit sits on `cb19f1a8`; `--ff-only`
would refuse). `origin/main` already has #723 (`2acae0e3`, rewrote the async `yield*` delegate machinery and added
`Interpreter::await_then` in `dispose.rs`). All line numbers below are **origin/main**; cite by function name.

- `src/interpreter/eval/generator_runtime.rs`
  - `async_generator_next_state_machine_impl`: the inline `Completion::Yield` block (≈3990–4045 on main) and the
    `StateTerminator::Yield` arm (non-delegate and delegate tails).
  - `yield_star_await_inner_result_resume` (the single `done` completion site, calls `bind_yield_star_result`).
  - `bind_yield_star_result` / `apply_sent_value_binding` (both ignore `InlineYield`; document why or handle it).
- `src/interpreter/eval.rs` — `Expression::Yield` arm (`yield*` branch: delete the async `get_async_iterator` +
  `await_value` branch, add the hand-off; reorder so the replay fast-forward check precedes evaluating the iterable).
- `src/interpreter/mod.rs` (+ `types.rs` if the discriminator lives on `GeneratorContext`) — the two small pieces of
  driver↔evaluator signalling described in slice 4.
- `test262-extra/` — new `async-generator-inline-yield-*.js` files (slices 1–5).
- Docs: new ADR `docs/adr/2026-09-22-HHMM-inline-yield-suspension.md` (same shape as
  `2026-09-21-2300-yield-star-delegated-step-suspension.md`: decisions + "Known boundaries"); `CONTEXT.md` — add an
  **Inline Yield** term (the fallback; what still replays, what no longer blocks); one sentence in `CLAUDE.md`'s
  generator Architecture Note that async-generator inline yields/`yield*` now suspend through the terminator machinery.

## 4. TDD slices

Run the RED tests against the pre-change release binary first and record the output in the PR description.
Every test in `test262-extra/` follows the existing pattern (`esid:`, `flags: [async]`,
`includes: [asyncHelpers.js, compareArray.js]`, `asyncTest(...)`; see
`async-generator-yield-star-next-step-await-does-not-nest.js`) and must be spec-derivable, not node-derived: the
ordering assertions compare the inline case's log against an otherwise-identical **lowered** (`yield 1` in statement
position) generator's log and additionally assert `after-next` precedes the first witness.

1. **Slice 1 (RED) — inline `yield v` does not nest.**
   `test262-extra/async-generator-inline-yield-await-does-not-nest.js`: `async function* g(){ var a; ({a = yield 1} =
   {}); }` vs lowered twin; witness chain of 6 started before `it.next()`; assert `after-next` is logged first and the
   two logs are equal. Fails today (`w1..w6` first).
   `…-inline-yield-pending-promise-suspends.js`: operand is a pending promise resolved by a later job; a queued second
   `it.next(x)` settles after the first, in order, and `a` receives `x` (checks the sent value still binds).
   `…-inline-yield-rejected-operand.js`: `yield Promise.reject(e)` inline rejects the request with `e` and completes
   the generator (parity with the terminator tail — see boundaries).
2. **Slice 2 (GREEN) — route the inline yield through the existing terminator tail.**
   *Precondition — settle the queue-pop question before editing.* Today the inline site does `resolve_fn(…)` →
   `drain_microtasks()` → `return Completion::Normal(promise)` with no pop, no `set_async_gen_yield_pending`, no
   `async_gen_process_queue`: the pop is done by the **caller** — `async_gen_process_queue` pops the front request and
   recurses whenever `is_async_gen_yield_pending()` is false after the driver returns. The terminator tail instead sets
   the flag (caller returns without popping) and pops inside its microtask/reaction, then calls
   `async_gen_process_queue`. So the swap is pop-neutral (one pop, moved into the continuation) **provided every
   caller of the driver honours the flag** — enumerate them (`async_gen_process_queue`, `async_generator_next`'s
   empty-queue path, `yield_star_await_inner_result_resume`'s `done` branch, the `async_gen_*resume` helpers) and
   confirm each does; write that conclusion in the PR description. This exact driver regressed
   `built-ins/AsyncGeneratorPrototype/return/request-queue-order-state-executing.js` in #712's experiment when a drain
   was removed without the queue state to replace it — run it, and `request-queue-order*`, right after this slice.
   Then, in the async driver, replace the
   `await_value` + resolve + `drain_microtasks` block: keep `stash_pending_iter_close` and
   `sync_generator_for_of_stack`, then build a synthetic `StateTerminator::Yield { value: None, is_delegate: false,
   resume_state: current_id, sent_value_binding: Some(InlineYield { yield_target: yield_count, prev_sent }) }` and let
   it fall into the existing `match &terminator` (same swap idiom as `inline_jump_terminator`), with a driver-local
   `inline_yield_operand: Option<JsValue>` consulted before `eval_operand(value)`. That reuses the tail's three paths
   (pending → `PerformPromiseThen` reactions; rejected → microtask reject; fulfilled → microtask resolve),
   `set_async_gen_yield_pending(true)` and per-request pop, and drops the nested drain. Preferred over adding an
   `await_then` continuation because it adds no new suspension code. Verify the tail's state store
   (`pending_exception/pending_return.take()`) is a no-op here and that `for_of_stack` is synced before the store.
   Slice-1 tests go green; run the 12 `for-await-of/async-gen-*-dstr-*yield*` families (they hit this path).
3. **Slice 3 (RED) — inline `yield*` in an async generator.**
   `…-inline-yield-star-async-iterable.js`: `({a = yield* inner()} = {})` with async-generator `inner` yields 10, 20;
   `a` ends `"r"` (today: `TypeError`).
   `…-inline-yield-star-does-not-nest.js`: `({a = yield 1, b = yield* [10]} = {})`, witness chain before the second
   `next("A")`; compare against the lowered `yield*` twin.
   `…-inline-yield-star-single-evaluation.js`: the iterable expression and `inner()`'s body prologue run exactly once
   across all steps (keep any preceding statement in the state idempotent — statement replay is a known follow-up).
4. **Slice 4 (GREEN) — hand `yield*` from the fallback to the delegate machinery.**
   - `eval.rs` `yield*` arm: when running under the async state-machine driver, do not iterate. Consult
     `generator_context` first: `current < target` ⇒ return `prev_sent_values[current]` **without** evaluating the
     iterable; `current == target` ⇒ evaluate the iterable once, bump `current_yield`, set a driver-visible flag and
     return `Completion::Yield(iterable)`. Sync generators keep the existing (await-free) loop unchanged.
   - Discriminator: `generator_context` is `None` on the first execution, so it cannot say "async". Add a small
     save/restore field set by the async driver next to `in_state_machine` (or always install a
     `GeneratorContext { is_async: true, target_yield: 0 }` in the async driver and `take()` it unconditionally —
     if so, check `exec.rs` `Statement::Return`'s `generator_context.is_none()` TCO guard, which is also gated by
     `!in_state_machine`). Avoid a bare global that a nested generator activation can clobber.
   - Async driver inline site: when the flag is set, build a synthetic `StateTerminator::Yield { is_delegate: true,
     resume_state: current_id, sent_value_binding: Some(InlineYield {…}) }` with `inline_yield_operand = iterable`
     and fall into the existing delegate arm (which already does `get_async_iterator`/sync fallback, the first `next`
     call, parks on `await_then` via `yield_star_suspend_on_inner_result`).
   - `yield_star_await_inner_result_resume`, `done` branch: if `deleg_info.sent_value_binding` is `InlineYield`, store
     `pending_binding: Some(that binding)` instead of `None` and keep passing `value` as the sent value into
     `async_generator_next_state_machine_with_promise`; the driver prologue already turns that into
     `target = yield_target`, `prev_sent.push(value)`, so the replay fast-forwards this `yield*` slot to the delegate's
     return value. `bind_yield_star_result` stays a no-op for `InlineYield` (comment says why).
   - Delete the async branch and `await_value` call from the `eval.rs` loop. Slice-3 tests go green.
5. **Slice 5 (RED→GREEN) — GC rooting.** Mirror `async-generator-yield-star-later-step-await-gc-rooting.js` for the
   inline yield operand and the delegated iterable across the await (whatever allocation-pressure idiom that file
   uses); a new `Interpreter` bool needs no GC handling, but the operand/iterable held only in Rust locals or a
   closure must be rooted for the duration of the await (`await_then` roots the awaited value; the tail's
   microtask vec roots its captures).
6. **Slice 6 (refactor/docs).** Remove the now-unused `_is_destructuring` read at the rewritten inline site; ADR,
   `CONTEXT.md`, `CLAUDE.md` note (§3). No formatting-only churn elsewhere.

**Decision gate.** If slice 4 needs more than the three touch points above (evaluator arm, driver inline site, the
single `done` completion site), stop after slice 2: land slices 1–2 + the audit in the ADR, use `Refs #710` (not
`Closes`) in the PR, and leave site (a) as a narrowed, documented residual with its repros in the issue.

## 5. Test surface

Targeted test262 (run before/after, must not regress):
`test262/test/language/statements/for-await-of/` (the `async-gen-*-dstr-*yield*` files are the ones on the changed
path), `language/statements/for-of/dstr/`, `language/statements/for-in/`, `language/expressions/async-generator/`,
`language/statements/async-generator/`, `language/expressions/yield/`, `language/expressions/assignment/dstr/`,
`built-ins/AsyncGeneratorPrototype/` (request-queue ordering — `request-queue-order*` /
`return/request-queue-order-state-executing.js` are the drain-sensitive ones), `built-ins/AsyncFromSyncIteratorPrototype/`.
Then the full `uv run python scripts/run-test262.py` (needs
`git submodule update --init --depth 1 test262 spec` in a fresh workspace) and
`uv run python scripts/run-test262.py test262-extra/`.
Not covered by test262 → `test262-extra/` (slices 1, 3, 5, each citing the clauses in §2 in `esid:`/`info:`):
microtask-ordering/non-nesting of the inline yield and inline `yield*`, async-iterable inline `yield*`, single
evaluation of the delegated iterable, rejected inline operand, GC rooting across the await.
Also `cargo test --release` and `./scripts/lint.sh`, `uv run python scripts/run-custom-tests.py`. Run the quality
gates as separate commands, never `&&`-chained. Build with `-j4`; do not rebuild while a suite run is in flight.

## 6. Regression risk

- `test262-pass.txt` should not move (no new passing test expected; the goal is no regressions). The exposed set is
  exactly the fallback's async hits: 26 for-await dstr scenarios plus any async-generator tests using destructuring
  defaults with `yield`.
- Shared machinery leaned on: the async-generator queue invariants from ADR-2026-09-21-2300 (a driver-context caller
  sets `async_gen_yield_pending` and returns; the continuation settles and pops the front request exactly once and
  then calls `async_gen_process_queue`) — the swap into the terminator tail must not double-pop or skip the pop.
  `stash_pending_iter_close` / `generator_inline_iters` / `for_of_stack` sync ordering at the inline site. GC rooting
  of values held across the await. The `InlineYield` replay prologue (`initial_inline_yield_*`).
- Behaviour changes to call out in the PR: inline `yield v` now costs the spec's one `Await` hop and stops draining
  the queue (request-queue ordering vs #712's load-bearing drains — re-run the two `request-queue-order*` tests
  first); inline `yield*` over an async iterable now works instead of throwing.
- Untouched on purpose, so no risk: sync driver, tree-walker/bytecode fast path (`eval_expr` `Yield` arm only),
  property MOP, `ObjectKind` matches.
- Merge risk: open PR #714 (drains on synchronous settle paths) edits the same driver; rebase before opening the PR.

## 7. Out of scope (file follow-ups via `gh issue create` after de-duplicating; link from the PR)

- Retire the fallback: lower destructuring patterns (assignment, binding, catch param, for-in/of heads) that contain
  suspensions into state-machine steps, then make `InlineYield` `unreachable!()` and delete `generator_context`
  replay (#625). Umbrella for the items below.
- `var/let/const {a = yield 1} = {}` and `catch ({a = yield 1})` swallow the yield (wrong value; node yields).
- `x[yield 1] = yield 2` evaluates the RHS suspension before the LHS one (`Assign` arm order).
- Replay re-executes preceding statements and the yield operand (`[2,2,7]` vs `[1,1,7]` repro above); plain-`yield`
  operand fast-forward ordering.
- Sync generator inline `yield*` hand-off (no blocking await there, but the same replay flaws).
- Legacy `IteratorState::AsyncGenerator` / `IteratorState::Generator` paths and their `await_value` (#711), `for
  await` step (#707/#685), `Expression::Await` in patterns (#709), `async_generator_await_return` drains (#712),
  rejected-await-at-`yield` should throw into the generator's own `try/catch` (both the terminator tail and this
  path complete the generator instead), `yield*` boundaries listed in ADR-2026-09-21-2300.
- Dead `destructuring_yield` flag cleanup beyond the one read removed in slice 6; formatting-only changes.
