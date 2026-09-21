# PLAN — issue #687: blocking `await_value` callers drain the microtask queue inline

Planning-stage artefact. The implementation stage `git rm`s this file before opening the PR.

## 1. Problem restated

`Interpreter::await_value` (`src/interpreter/eval.rs:9839`) is a *blocking* Await: it registers a
reaction, then runs the microtask queue **inline, nested inside the current JS call** until its
own continuation fires. Its callers therefore run every already-queued job in the middle of
synchronous code, and the enclosing promise settles at the wrong tick. #665/#699/#701 moved
`await using`, `for (await using … of …)` and block scopes onto the suspendable state machine; the
remaining `await_value` callers were split out here for an audit. The audit (below) found that the
same defect class has a second, larger member the issue text does not name:
`Interpreter::drain_microtasks()` is called ~55 times in `generator_runtime.rs` (plus `eval.rs:8094`)
right after settling a request/result promise, so `it.throw("E")`, `it.return(3)`,
`Async-generator-that-throws.next()` and `async function f(a = throw){}` run *all* queued jobs
before returning the promise to a synchronous caller.

The task is an **audit plus a minimal first fix**, not a rewrite of the async-generator driver.

## 2. Spec basis

Cite by heading/anchor in `spec/spec.html` (numbers drift; the submodule is authority — the
`spec/` checkout in a fresh workspace is empty: `git submodule update --init --depth 1 spec`).

* **Jobs and Host Operations to Enqueue Jobs** (`#sec-jobs`): a Job runs "when there is no running
  context in the agent … and that agent's execution context stack is empty"; "Once evaluation of
  a Job starts, it must run to completion before evaluation of any other Job starts". A nested
  drain from inside a running call violates both bullets. Also `#sec-hostenqueuepromisejob`.
* **TriggerPromiseReactions** (`#sec-triggerpromisereactions`) / **NewPromiseReactionJob**
  (`#sec-newpromisereactionjob`): settling a promise only *enqueues* reaction jobs; it never runs
  them.
* **Await** (`#await`): the continuation is a `PerformPromiseThen` reaction, resumed by a later Job
  after the running execution context is suspended — never synchronously.
* **%AsyncGeneratorPrototype%.next / .return / .throw** (`#sec-asyncgenerator-prototype-next`,
  `-return`, `-throw`), **AsyncGeneratorValidate**, **AsyncGeneratorEnqueue**,
  **AsyncGeneratorStart**, **AsyncGeneratorCompleteStep**, **AsyncGeneratorResume**,
  **AsyncGeneratorYield**, **AsyncGeneratorAwaitReturn**, **AsyncGeneratorDrainQueue**
  (`#sec-asyncgeneratorvalidate` … `#sec-asyncgeneratordrainqueue`): an invalid receiver produces
  a rejected promise via `IfAbruptRejectPromise`, a `return`/`throw` on a suspended-start or
  completed generator settles via AsyncGeneratorAwaitReturn's single `PerformPromiseThen` — all
  return the promise to the caller with no job run in between.
* **EvaluateAsyncFunctionBody** (`#sec-async-function-definitions-…`, the `AsyncFunctionBody :
  FunctionBody` algorithm near `spec.html:25410`): if `FunctionDeclarationInstantiation` is
  abrupt, `Call(promiseCapability.[[Reject]], …)` then *return the promise* — no drain.
* **Runtime Semantics: Evaluation** for `YieldExpression : yield * AssignmentExpression`
  (`#sec-generator-function-definitions-runtime-semantics-evaluation`; code cites it as §15.5.5):
  in async generators `innerResult` is `? Await(innerResult)` at each step, i.e. an Await tick per
  delegated step (follow-up F2).
* **for-await** (`ForIn/OfBodyEvaluation`, async iteration kind): `Await(nextResult)` per step
  (follow-up F1).

## 3. Audit — every blocking `await_value` caller and every settle-then-drain site

Legend for disposition: **S#** = slice in this PR, **F#** = follow-up issue the implementation
stage files (`needs-triage`), **T** = already tracked elsewhere.
Repros use the witness-chain shape (a `Promise.resolve().then(w1)…then(w6)` chain started before
the call; `L` logs; `sync-end` logged right after the call). Node output is the oracle only where
it agrees with the spec text above.

### 3a. `await_value` callers

| # | Site | Reached by | Repro (jsse ≠ node) | Disposition |
|---|------|-----------|---------------------|-------------|
| 1 | `eval.rs:1019` `Expression::Await` (tree-walker) | `await` in a *pattern default* (`var {a = await 1} = {}`, `[a = await 1] = []`, nested patterns, `for (var {a = await 1} of …)`, `catch ({a = await 5})`, async-generator bodies); nothing else reaches it — `generator_transform.rs` only scans `declarator.init` (`:1863`, `:2154`), never the pattern's `Initializer`s | `w1,a1,sync-end,…` vs `sync-end,w1,a1,…` | **F3** (needs pattern lowering — not small) |
| 2 | `eval.rs:933` `yield*` in the `generator_context` InlineYield fallback (async gen only) | degraded backstop path (CLAUDE.md, issue #625); not reproduced | none found | **F4** (audit-only: prove unreachable or convert) |
| 3 | `exec.rs:2217` `exec_for_of_loop` `is_await` step | `for await` executed by the tree-walker: nested in `if`/`try`/loop body (`if (true) { for await … }`) | `if_forawait`: `w1,…,end,sync-end` | **T** — #685 (nested-container gap) |
| 4 | `dispose.rs:251` `run_dispose_cursor_blocking` | `await using` in loop/switch/generator sites still on blocking driver | — | **T** — #685 / #686 |
| 5 | `generator_runtime.rs:3098` (`yield_star_return_after_unwrap`), `:3321` (delegated `.return()`), `:3504` (delegated `.throw()`), `:3718` (delegated `.next()`) | second and later steps of `yield*` in an async generator (the first step already uses the non-blocking `yield_star_await_inner_result_resume`, `:4440-4540`) | `async function* g(){ yield* inner() } it.next().then(()=>{ it.next()… })`: `after-next2` never observed before `w6` (nested drain) | **F2** |
| 6 | `generator_runtime.rs:4252` | inline (non-lowered) `yield v` in an async generator awaits `v` blocking | — | **F4** |
| 7 | `generator_runtime.rs:5594` ForOfHead `is_await` in `async_generator_next_state_machine_impl` | **`for await` directly in an async generator body** | `async function* g(){ L("body"); for await (var y of [1]) L("y"+y) } it=g(); it.next(); L("after-next")`: `body,w1,w2,y1,…,after-next,sync-end` vs `body,after-next,sync-end,w1,w2,y1` | **F1** (port `async_function_resume`'s ForOfHead suspension) |
| 8 | `generator_runtime.rs:6128`, `:6166` | legacy `IteratorState::AsyncGenerator` path; the only constructor of that variant is the legacy function itself (grep: no initial-state creator), so the path is **dead** | none | **F5** (delete dead legacy path, with `async_generator_return_legacy`/legacy `throw`; refactor, not bundled) |

### 3b. `drain_microtasks()` after settling a promise that is returned to a synchronous caller

| # | Site | Repro (jsse ≠ node) | Disposition |
|---|------|---------------------|-------------|
| 9 | `generator_runtime.rs:2489` `reject_with_type_error` (invalid receiver of `next/return/throw`) | `N.call({}).catch(()=>L("rej")); L("after-call")`: `w1..w6,after-call,sync-end,rej` vs `after-call,sync-end,w1,rej,w2…` | **S1** |
| 10 | `eval.rs:8094` async-function parameter-binding rejection | `async function f(a = (()=>{throw 1})()){}; f().catch(…); L("after-call")`: `w1..w6,after-call,…` | **S2** |
| 11 | `generator_runtime.rs:6357` (`async_generator_return_state_machine_with_promise`), `:6463`, `:6476` (`…throw_state_machine_with_promise`), `:6252`, `:6285` (`async_generator_await_return`) | `it=g(); it.throw("E").catch(…); L("after-throw")` and `it.return(3).then(…)` at suspended-start/completed: `w1..w6,after-throw,…` | **S3** |
| 12 | ~42 drains inside `async_generator_next_state_machine_impl` (`:3334`–`:5789`) after settling on body-throw / completion / yield | `async function* g(){ throw 1 } g().next().catch(…); L("after-next")`: `w1..w6,after-next,…` | **S4 (gated experiment)**; anything not clean → **F6** |
| 13 | `generator_runtime.rs:6074/6080/6140/6178/6225`, `:6560`, `:6630` (legacy `async_generator_next/return/throw`) | dead (see #8) | **F5** |
| 14 | `eval.rs:1122` (deferred `import()`), `eval/modules.rs:882`, `builtins/mod.rs:7970` (ShadowRealm / agent broadcast), `mod.rs:5686/5703/5722/5764/5906`, `mod.rs:2355-2389` | top-level event-loop drains or host-boundary drains, not user-visible nested drains from a JS call frame (`eval("0")` verified clean) | benign — leave; note in PR |

## 4. Files to touch

* `src/interpreter/eval/generator_runtime.rs` — S1, S3, S4 (remove drains on synchronous return paths).
* `src/interpreter/eval.rs` — S2 (`~:8094`, the parameter-binding rejection path).
* `test262-extra/` — new files, one per slice (names in §5), copying the frontmatter shape of
  `test262-extra/async-disposable-stack-dispose-async-tick-alignment.js` (`flags: [async]`,
  `includes: [asyncHelpers.js, compareArray.js]`, `esid`, `description`, `info` quoting the clause).
* No `docs/adr/` entry and no `CONTEXT.md` change: this fixes drift from existing spec text and
  introduces no new abstraction. (If S4 lands, add a one-line note under the async-generator section
  of `docs/architecture.md` that the queue driver never drains the microtask queue inline.)
* Not touched: `spec/`, `test262/`, `test262-pass.txt`, `generator_transform.rs`,
  `generator_analysis.rs`, `exec.rs`, `dispose.rs`.

## 5. TDD slices (red → green → refactor; one commit each)

Ground every expected witness log in the spec clause above *and* cross-check with `node`; if they
disagree, the spec text wins (record the disagreement in the PR). Run each new test red on the
current binary first; keep each test's `sync-end` position and reaction order as the assertion.

1. **S1 — invalid receiver rejects without draining.**
   Test: `test262-extra/async-generator-prototype-invalid-receiver-does-not-drain-microtasks.js`
   (esid `sec-asyncgenerator-prototype-next`, also covers `return`/`throw`): witness chain started
   before `next.call({})`, `.return.call(1)`, `.throw.call(null)`; assert `sync-end` precedes `w1`.
   Green: delete the `self.drain_microtasks()` in `reject_with_type_error` (`:2489`).
2. **S2 — async-function parameter throw does not drain.**
   Test: `test262-extra/async-function-parameter-binding-throw-does-not-drain-microtasks.js`
   (esid `sec-async-function-definitions-…` / `AsyncFunctionBody : FunctionBody`), plain async
   function, async arrow, async method (`async function f(a = (()=>{throw 1})()){}`).
   Green: delete the drain at `eval.rs:~8094`. (Confirm the resolve/reject on the *success* side of
   the same function has no drain either; if it does, remove and note.)
3. **S3 — `return`/`throw` on suspended-start / completed async generators.**
   Test: `test262-extra/async-generator-return-throw-at-start-does-not-drain-microtasks.js`
   (esid `sec-asyncgenerator-prototype-return`, `sec-asyncgeneratorawaitreturn`): `it.throw("E")`
   and `it.return(3)` on a fresh generator and on a completed one, plus a pinned tick count
   for the `return` result promise (AsyncGeneratorAwaitReturn's one `PerformPromiseThen` hop, then
   the request promise's reaction).
   Green: drop the drains at `:6252/:6285/:6357/:6463/:6476` after settle. Verify tick counts do
   not regress on the existing `built-ins/AsyncGeneratorPrototype/return/*` tests.
4. **S4 (gated) — body-completion settle drains in `async_generator_next_state_machine_impl`.**
   Test: `test262-extra/async-generator-body-throw-and-completion-do-not-drain-microtasks.js`
   (esid `sec-asyncgeneratorcompletestep`, `sec-asyncgeneratordrainqueue`): a generator whose body
   throws on the first `next()`, one that returns on first `next()`, and the value-yield case,
   all called both from top-level code and from inside a job.
   **Experiment first, uncommitted**: strip *only* the `settle → drain → return Completion::Normal(promise)`
   drains, run `built-ins/AsyncGeneratorPrototype/`, `language/*/async-generator*/`,
   `language/statements/for-await-of/` and the `y1.js`/`y2.js`-shaped probes (`yield*` and
   `it.return(5)` chains, which currently only finish at a timer boundary). Decision rule: if
   anything hangs or regresses and the cause is not immediately obvious, **abandon S4** (keep
   S1–S3), write the findings into follow-up F6 and stop. Only commit S4 if the full suite is
   clean and the probes settle within the microtask queue.
5. **Audit deliverable** (no code): post the §3 table as an issue comment on #687 with the
   repros; file follow-ups F1–F7 (`gh issue create`, label `needs-triage`, each linking #687 and
   quoting its repro). Do this before the PR is opened so the PR body can name them.

## 6. Test surface

* Targeted test262 (must stay green, `uv run python scripts/run-test262.py <dir>`):
  `test262/test/built-ins/AsyncGeneratorPrototype/`, `built-ins/AsyncGeneratorFunction/`,
  `built-ins/AsyncFunction/`, `language/expressions/async-generator/`,
  `language/statements/async-generator/`, `language/expressions/yield/`,
  `language/statements/for-await-of/`, `language/expressions/await/`,
  `language/statements/async-function/`, `language/expressions/async-function/`,
  `language/expressions/async-arrow-function/`, `language/expressions/class/` and
  `language/statements/class/` (the `async-gen-method*` dstr/params cases), `built-ins/Promise/`.
  These assert final values, not interleaving, so they are regression gates, not the proof.
* `test262-extra/`: the four new files in §5 are the proof — tick-order is not covered by test262.
  Run: `uv run python scripts/run-test262.py test262-extra/<file>` (no dedicated runner; see
  memory note on running test262-extra).
* Full suite (`uv run python scripts/run-test262.py`, baseline from `origin/main`; fresh workspaces
  need `git submodule update --init --depth 1 test262 spec` first), `uv run python
  scripts/run-custom-tests.py`, `cargo test --release`, `./scripts/lint.sh` — run as separate
  commands, never `&&`-chained. Build with `cargo build --release -j8` (bounded; ~1.5 min) and
  never rebuild while a suite run is in flight.

## 7. Regression risk

* **The baseline can only move by *failing* tests turning up**, never by passes appearing: the
  change removes work, it adds none. Watch specifically for `AsyncGeneratorPrototype` tests
  that assert relative order between a rejected `next()` and later promise callbacks.
* **Load-bearing drains (the main risk).** The `y1.js`/`y2.js` probes show that async-generator
  chains involving `return()` / `yield*` currently only complete at a timer boundary, so *some*
  drains inside the queue driver may be what makes forward progress happen (the settle drain runs
  the reaction that pops the queue / resumes the next request via `async_gen_process_queue` and
  `is_async_gen_yield_pending`). That is why S1–S3 are limited to paths that return a promise to a
  synchronous caller with no continuation depending on the drain, and S4 is an experiment with
  an abort rule. A hang, not a reorder, is the failure mode to look for: run every touched
  directory with the default 120 s limit and inspect any timeout.
* Shared machinery leaned on: `Scheduler::async_gen_queue*`, `is_async_gen_yield_pending`,
  `create_resolving_functions`, `perform_promise_then`, `gc_root_frame`/GC safepoints (no new
  rooting is introduced; removing a drain removes a `gc_unroot_frame` interleaving only). No
  change to `eval_expr`/`exec_statement` hot paths, `property.rs`, `ObjectKind`, or the bytecode VM.
* Node-compat library harnesses (`acorn`, `decimal.js`, …) use async code lightly; not expected to
  move, but run `./scripts/run-library-tests.sh acorn` if S4 lands (async-heavy tick order).
* Tick-order changes are *observable*: any code that relied on the accidental early delivery
  (e.g. `Promise.race` timing tests in `test262-extra/`, `tests/`) will show in
  `run-custom-tests.py`; fix by correcting the expectation to the spec-derived order, never by
  restoring a drain.

## 8. Out of scope (follow-ups to file, not bundled)

* **F1** `for await` inside an async generator body (`:5594`) — port the async-function ForOfHead
  suspension to the async-generator driver (the flagship `await_value` blocking repro).
* **F2** `yield*` delegated resume steps (`:3098/:3321/:3504/:3718`) — factor the
  promise-resolve/`perform_promise_then`/`yield_star_await_inner_result_resume` block at
  `:4440-4540` into a helper and reuse it.
* **F3** `await` inside destructuring defaults (`eval.rs:1019`) — lower pattern `Initializer`s in
  `generator_transform.rs` (currently only `declarator.init` is scanned).
* **F4** audit `eval.rs:933` and `generator_runtime.rs:4252` (InlineYield fallback): prove
  unreachable or convert.
* **F5** delete the dead legacy `IteratorState::AsyncGenerator` path
  (`async_generator_next/return/throw` legacy bodies, `:6027`–`:6700`) — pure refactor.
* **F6** remaining queue-driver settle drains if S4 is abandoned or partial.
* **F7 — different bug family, do not fix here (wrong values / lost completions):**
  `delete o[await "a"]` → `false` (Node `true`); `o[await "k"]++` does not increment;
  `o[await "k"] = await 5` and `({a = await 1} = {})` never run the following statement (the
  async function silently dies); `null ?? await 3` → `undefined`. Likely an un-lowered
  `await` `Completion::Yield` swallowed by operator/assignment evaluators. File as a new
  `needs-triage` correctness issue with those five repros; they are strictly worse than a tick shift.
* Already tracked, excluded: `if (…) { for await … }` nested-container gap (#685), `await using`
  in async generators (#686).
* Also not bundled: any change to `await_value` itself, `drain_microtasks_until_idle`, the
  event-loop drains in `mod.rs`, formatting/cleanup of touched files.

## 9. PR

* Title: `fix(generators): stop draining the microtask queue on synchronous async-generator/async-function settle paths` (Conventional Commits; squash subject is taken verbatim).
* Body: `Refs #687` (**not** `Closes`): the issue title's `await_value` callers themselves are
  still open follow-ups F1–F4; the PR lands the audit and the sync-return drain fixes. State the
  spec clauses, list F1–F7 with their numbers, and note the S4 outcome (landed / abandoned and why).
* Remember: convert this plan to a task list (TaskCreate) before executing; `git rm PLAN.md`
  before opening the PR; do not run `--update-baseline`.
