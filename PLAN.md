# Plan: issue #716 — async generators: remaining blocking disposal / unwind paths after #686

Line numbers are for `src/interpreter/eval/generator_runtime.rs` at `997eaee1` unless a file is named.

## 1. Problem restated

The async-generator driver (`async_generator_next_state_machine_impl` and its continuations) still has six places
that drain the microtask queue inline or complete the generator without the spec's suspension, listed in #716.
The issue body claims "none of which involve a wrong *value* today". **That is false for two of them** (probed on a
fresh `997eaee1` release build, cross-checked against Node 26 as a reference only):

| Item | Probe | jsse | Node / spec | Kind |
|---|---|---|---|---|
| 3 | `try{yield 1}catch(e){yield 'c'}` then `it.return(Promise.reject('E'))` | request rejects `E`, `catch` never runs | `catch` runs, generator yields `'c'` | **wrong value** |
| 5 | fn-level `await using` + parked in `yield*`, then `it.return('R')` | disposer **never runs** | disposer runs before the request settles | **missing side effect** |
| 3 | `try{yield 1}finally{L('fin')}` then `it.return({then(r){L('then');r(7)}})` | `fin,then` | `then,fin` | order |
| 1 | `for (await using a of [x]) …` in an async generator | disposal drains inline: `after` logged before `sync-end` | after | tick |
| 2b | `try{ {await using a…; yield 1; return 5} }finally{…}` | `disp,fin` run inside the `it.next()` call, before the caller's next statement | after | tick |
| 2a | `.throw('T')` at a yield inside `try{ {await using a…; yield 1} }catch{}` | `dispA,caught` before caller's next statement | `dispA` before, `caught` after | tick |
| 4 | `it.return(pendingPromise); it.next()` on a suspended-start generator | `next` settles first | `return` then `next` | order (tracked by #712) |

Items 1, 2a, 2b are tick-only (values match Node in every shape probed, including three where a throwing disposer
replaces an in-flight `return`). Items 3 and 5 are value-visible. Items 4 and 6 are already owned by other issues/PRs.

Repro scripts live only in `$TMPDIR` (cleaned per attempt); recreate from the appendix.

## 2. Spec basis

ECMA-262 clauses (by id in `spec/spec.html`; on this pin `sec-asyncgeneratorunwrapyieldresumption` is ~L50772):

- `sec-asyncgeneratorunwrapyieldresumption` — a return resumption **first** `Await`s its value; a throw completion
  from that Await is thrown *into the generator at the yield* (`return ? awaited`). Basis for item 3.
- `sec-asyncgeneratoryield` — calls UnwrapYieldResumption on the queued or resumed completion.
- `sec-asyncgeneratorstart` (step "If _result_ is a return completion, set _result_ to NormalCompletion(value)") —
  the body's completion is **not** awaited again when the generator finishes; `sec-return-statement-runtime-semantics-evaluation`
  is where `return expr` awaits.
- `sec-asyncgeneratorawaitreturn` / `sec-asyncgeneratordrainqueue` — the `draining-queue` state (item 4 → #712, excluded).
- `sec-generator-function-definitions-runtime-semantics-evaluation` (`YieldExpression : yield * AssignmentExpression`):
  the `received` return arm (`GetMethod(iterator,"return")`; undefined → `Await(receivedValue)` then
  `ReturnCompletion`; else `Await(innerReturnResult)`, `done` → `ReturnCompletion(IteratorValue)`). The
  ReturnCompletion propagates through the body, so its DisposeResources runs. Basis for item 5. A throw `received`
  (rejected Await in UnwrapYieldResumption) calls the inner iterator's `throw`.
- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset` step 9.k.ii — a
  `for (await using …)` iteration environment is disposed with DisposeResources(iterationEnv, result). Basis for item 1.
- Explicit Resource Management is **not** in the `spec/` pin (`grep DisposeResources spec/spec.html` is empty). Cite it as
  `proposal-explicit-resource-management, sec-disposeresources` (the convention in
  `test262-extra/async-generator-await-using-return-at-yield-disposes.js`), never as a `spec/` clause number. Its
  DisposeResources step 3.d/4 `Await`s are what the `DisposeCursor` (`src/interpreter/dispose.rs`) already models.

`spec/` and `test262/` are empty submodules in a fresh workspace and are read-only. A populated read-only copy is at
`/home/pmatos/dev/jsse/{spec,test262}`; otherwise `git submodule update --init --depth 1 spec test262`.

## 3. Scope decision (documented, per the operating contract)

**In this PR (in priority order, each an independent, green, committable slice):** items 5, 3, 2a/2b, 1.

**Out of scope, with owners** (post as one `gh issue comment 716` at PR time; the PR body says `Part of #716`, never
`Fixes`, because the items below stay open):

- **Item 4** (`AsyncGeneratorAwaitReturn` must keep the queue blocked; `.return(v)` at suspended-start/completed drains
  inline) → **#712**, whose body already specifies the fix and names the guard test
  `test262-extra/async-generator-request-queue-return-throw-next-fifo.js`.
- **Item 6** (blocking `await_value` in `yield*` delegated calls, the inline `Completion::Yield` operand, `for await`
  steps) → **#687 / #707 / #710**, with open PRs **#714, #720, #729** editing this same file.
- **Item 2, inline-replay part** (`is_inline_replay` in the frame-leave loop, ~3847) → the `InlineYield` fallback is being
  replaced by #729; re-check on the rebased base and delete the `!is_inline_replay` term if it is gone.
- **Item 2, `for-of` unwind part** (`dispose_scopes_inside_for_of` L6431 and `close_for_of_loop`'s
  `iteration_env` disposal, `eval.rs` ~9655): needs a *resumable* unwind across five callers
  (`route_generator_exception`, `route_generator_loop_control`, the `pending_return` block, both `Return` terminator
  arms, `align_generator_for_of_stack`). Design sketch for the follow-up: make `unwind_generator_for_of_loops` return
  `Done(Completion) | Parked`, leave each loop on `for_of_stack` with `iteration_env` `take()`n in place before parking
  (so a re-run of the same routing on resume is idempotent), and re-invoke the routing with the in-flight completion
  restored, exactly as `async_function_resume` re-invokes `route_return!`. Too large to bundle here.
- Delegated-`yield*` abrupt exits that skip `finally`/outer `for-of` closing, and rejected inner results that reject the
  request instead of throwing into the body's `try`/`catch` (ADR-2026-09-21-2300 "Known boundaries"). Design sketch:
  deliver the delegation's completion into the body as `pending_return`/`pending_exception` at `resume_state` and
  let the ordinary unwinding run. Not needed for item 5 and it moves ticks pinned by test262 (`yield-star-*-ticks`).
- The ADR's "throwing disposer replaces an in-flight `return` and does not re-enter a `finally` already selected" shape:
  **not reproduced** (probes below matched Node). Do not plan a fix for an unwitnessed defect; if the implementer finds a
  witness while doing slice 3, add the red test first, otherwise leave the ADR sentence as-is and say so in the PR.

## 4. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — all four slices.
- `src/interpreter/dispose.rs` — only if `GeneratorDisposeThen` needs a doc-comment update (slice 2 changes what
  `ReturnAwait` is used for). No new variant is expected.
- New tests under `test262-extra/` (names in §6) and, only if an engine-internal observable is not JS-visible,
  `tests/`.
- `docs/adr/2026-09-22-<HHMM>-async-generator-return-operand-and-parked-unwind.md` (new ADR; do not rewrite the
  accepted 2026-09-21 ADRs except to append a one-line "superseded in part" pointer under their "Known boundary" text,
  the convention ADR-2026-09-21-1007 already uses).
- `CONTEXT.md` — **Frame-Exit Disposal** entry: replace the last sentence ("A disposal reached while a throw or return is
  already in flight … falls back to the blocking driver") with the post-slice-3 truth (only inline replay and the
  `for-of` unwind remain blocking).

Nothing under `spec/`, `test262/`, `test262-pass.txt`, or `Cargo.toml`.

## 5. TDD slices

Preflight (once): `git submodule update --init --depth 1 test262`; `git fetch origin main` and check whether #714 / #720 /
#729 have merged (`gh pr view <n> --json state`), rebasing first if they have (they edit this file; slice 4 sits ~70 lines
from #720's `await_value(&step_result)` edit in the same `ForOfHead` arm). Build with a capped job count
(`cargo build --release -j8`, `CARGO_PROFILE_RELEASE_DEBUG=0`, target dir under `$TMPDIR`), then **snapshot the baseline**:
`cp <target>/release/jsse $TMPDIR/jsse-base`, and run the targeted test262 directories in §6 once with it, saving the
pass lists, so any regression can be diffed against pre-change behaviour. Never rebuild while a suite run is in flight.
Run lint, `cargo test --release`, and each test262 command as **separate** commands.

Every slice: write the `test262-extra/` test, run it against `$TMPDIR/jsse-base` and confirm it fails **for the stated
reason**, then implement, then run the slice's targeted dirs. Commit each slice separately (Conventional Commits; the PR is squash-merged, so the
branch history only needs to be clean).

Note the ordering constraint: the Edit/Write hook runs `clippy -D warnings`, so a slice must not leave a
`GeneratorDisposeThen` variant unconstructed. Slice 1 adds new `ReturnAwait` constructions *before* slice 2 removes the
old one.

### Slice 1 — item 5: `.return(v)` parked in `yield*` disposes function-level / open-frame resources

Red test: `test262-extra/async-generator-yield-star-return-disposes-function-level-resource.js`
(esid `sec-generator-function-definitions-runtime-semantics-evaluation`; info cites the proposal's `sec-disposeresources`).
Shapes, each asserting the disposer ran exactly once **before** the `.return()` promise settles and that a
`.next()` queued behind it settles after: (a) inner iterator with a `return` returning `{done:true}` (the
`done && step == DelegateStep::Return` arm, ~2958); (b) inner iterator with **no** `return` method (the delegated
`pending_return` `Ok(None)` arm ~3323 and `yield_star_return_after_unwrap`'s `Ok(None)` arm ~3196); (c) async disposer
that takes ticks, using the tick-witness pattern of `async-generator-await-using-fn-level-suspends-at-dispose.js`;
(d) an open `await using` block frame around the `yield*` (covered by `take_generator_dispose_stack`, L5540).
Green: the three arms currently mark the generator completed and call `async_generator_await_return` directly. Route them
through `self.async_gen_dispose(gen_id, &func_env, Completion::Return(value), GeneratorDisposeThen::ReturnAwait,
(&promise, &resolve_fn, &reject_fn))` (L5589):
- `Done(completion)` (no resources): continue with today's tail unchanged (mark completed, `async_generator_await_return`,
  pop, `async_gen_process_queue`) — tick-neutral for resource-free generators. Call `sync_generator_scope_stack(id, &[])`
  as `dispose_or_park!` does.
- `Parked`: return. In the driver-context arm (~3323) that means `set_async_gen_yield_pending(true); return
  Completion::Normal(promise)`; in the two job-context arms, no flag (ADR-2026-09-21-2300 queue invariants: a job-context
  continuation does not set it, and `async_gen_finish_disposal` settles/pops/processes exactly once).
Verify `func_env` is in scope on each path (it is, from the cloned `IteratorState`) and that the parked
`GeneratorDisposal` is GC-rooted (`generator_pending_dispose` is already a root); add
`async-generator-yield-star-return-dispose-suspended-gc-rooting.js` modelled on
`async-generator-await-using-dispose-suspended-gc-rooting.js`.
Fold the *reject* arms (rejected inner result, non-object, `IteratorComplete`/`IteratorValue` throw, `iterator_return` Err —
they skip DisposeResources too) into the same helper **only** if each is a one-line change with `Settle`; otherwise list
them as a follow-up. Do not touch `finally`/`for-of` closing here (§3).

### Slice 2 — item 3: `.return(v)` at a yield awaits `v` *before* unwinding

Red tests (esid `sec-asyncgeneratorunwrapyieldresumption`):
- `async-generator-return-at-yield-awaits-operand-before-unwinding.js`: thenable operand → `then` runs before the
  generator's `finally`; a pending-promise operand keeps the `finally` from running until it settles.
- `async-generator-return-at-yield-rejected-operand-enters-catch.js`: the `try{yield}catch{yield 'c'}` probe (the
  request resolves `{value:'c',done:false}`), plus the no-`catch` shape (rejected operand → `finally` runs, request rejects
  with the reason), plus `return(brokenPromise)` where `constructor` throws (existing synchronous pre-check must keep
  its tick count; `built-ins/AsyncGeneratorPrototype/return/return-suspendedYield-broken-promise-try-catch.js` pins it).
- delegated: `.return(rejectedPromise)` parked in `yield*` calls the inner iterator's `throw` (spec `received` is a throw
  completion), not `return`.
Green (`async_generator_return_state_machine_with_promise`, L5979, the `SuspendedAtState` tail L6069–6091): keep the
synchronous broken-`constructor` pre-check; replace "store `pending_return = Some(value)` and run" with
`await_then(&value, …)` (`dispose.rs` L355) leaving the generator `SuspendedAtState` (keep `delegated_iterator`), the
request at the queue head, and `set_async_gen_yield_pending(true)`. The continuation writes `pending_return = Some(v)` on
fulfilment or `pending_exception = Some(e)` on rejection, then re-enters
`async_generator_next_state_machine_with_promise` and pops/processes with the **inline** convention of
`yield_star_await_inner_result_resume` (not `async_gen_await_resume`, which defers `async_gen_process_queue` by a
microtask). Factor the shared re-enter-and-settle epilogue rather than adding a third copy.
Then `pending_return` is always an already-awaited value, so in the driver's `pending_return` block (L3777) switch
`GeneratorDisposeThen::ReturnAwait` to `Settle` and resolve `{value, done:true}` directly (spec: no second await). The
`Return` terminator with a `finally` (L4452, L4622) and the inline `Completion::Return` (L3988) also call this function;
they now get the single `Await(exprValue)` of `return expr;` before the `finally` runs, which is spec-correct.
`ReturnAwait` stays only for slice 1's delegated arms (keep the doc comment in `dispose.rs` truthful: the
`Await(receivedValue)` of the no-`return` arm is a genuine second await; the `done` arm's extra await is a known
one-tick deviation, unchanged).

### Slice 3 — item 2a/2b: frame disposal with an in-flight throw/return parks

Red tests: `async-generator-await-using-block-exit-inflight-throw-suspends.js` (the `.throw('T')` probe, plus two nested
frames whose inner disposer throws) and `…-inflight-return-suspends.js` (the `return 5` through `try/finally`
probe and `.return('R')` at a yield in a block inside `try/finally`), each with a synchronous-caller witness
(`sync-mid` logged after the `it.next()/throw()/return()` call must precede the disposer's continuation) and one throwing-disposer
case asserting the `SuppressedError` chain and that the `finally` selected for the return runs **exactly once**.
Extend `async-generator-await-using-inflight-completion-gc-rooting.js` with a *parked* (not blocking) case.
Green (frame-leave loop, L3835–3904): drop `pending_exception.is_none() && pending_return.is_none()` from `can_park`
(keep `!is_inline_replay` until #729 is resolved). Seed the `DisposeCursor` with `Completion::Throw`/`Completion::Return`
(as the blocking path already does), and when parking store `pending_exception: None, pending_return: None` in the saved
state — the cursor owns the in-flight completion (`for_each_value` roots it). Extend `async_gen_reenter_after_disposal`
(L5747) to restore a finished `Completion::Return(v)` as `pending_return` (today it restores only `Throw`); a finished
`Throw` already becomes `pending_exception` and re-routes as a fresh exception, which the probes show is value-correct.
Re-entry re-runs the routing against the already-truncated `try_stack`; verify idempotence by test (handler re-found, no
double `finally`).

### Slice 4 — item 1: `for (await using x of …)` head disposal parks (rebases onto #720)

Red test: `async-generator-await-using-for-of-head-dispose-tick-alignment.js`, reusing the shapes and witness numbers of
`await-using-for-of-head-dispose-tick-alignment.js` (null resource → no Await; async disposer → one Await; throwing
disposer → iterator closes once and the throw reaches the generator's `catch`), and
`async-generator-await-using-for-of-head-dispose-suspended-gc-rooting.js` modelled on
`await-using-for-of-head-dispose-suspended-gc-rooting.js`.
Green (`ForOfHead` arm, `dispose_resources(&iteration_env, …)` at ~5112): `take_dispose_stack(&iteration_env)`;
none → skip; else `DisposeCursor::new(stack, Completion::Empty)` and `step`. `Done(Throw(e))` → set
`pending_exception = Some(e); check_abrupt_on_resume = true; continue` (the loop stays on `for_of_stack` with
`iteration_env` already `take()`n, so `route_exception!` → `unwind_generator_for_of_loops` closes the iterator once with
the throw preserved — replacing the inline `iterator_close` + `remove(loop_pos)` + route sequence). `Await` → `sync_generator_for_of_stack`, store
`SuspendedAtState { state_id: current_id }` with the current `try_stack`, park with `GeneratorDisposeThen::Reenter`,
`set_async_gen_yield_pending(true)`. `Exit` handling unchanged. Confirm first that a `for (await using …)` head is lowered in
async generators (`awaits_at_head()` in `stmt_has_suspension`); if not, that is a lowering bug to state in the PR, not to
paper over. If #720 has merged, rebase before starting this slice.

### Slice 5 — docs

New ADR (decisions: `.return` operand awaited before unwind, reject → throw at the yield; delegated-return disposal via
`ReturnAwait`; parked frame disposal carries the in-flight completion in the cursor; remaining boundaries: `for-of`
unwind, inline replay, delegated `finally`/`for-of`). Update `CONTEXT.md` **Frame-Exit Disposal** and the "Known
boundary" sentences of ADR-2026-09-21-2015 / -2300 with a superseded-in-part pointer.

## 6. Test surface

New `test262-extra/` files (test262 front-matter, `flags: [async]`, `includes: [asyncHelpers.js, compareArray.js]`,
`features: [explicit-resource-management, async-iteration]`, esid as named above):
`async-generator-yield-star-return-disposes-function-level-resource.js`, `…-return-dispose-suspended-gc-rooting.js`,
`async-generator-return-at-yield-awaits-operand-before-unwinding.js`, `…-rejected-operand-enters-catch.js`,
`async-generator-await-using-block-exit-inflight-throw-suspends.js`, `…-inflight-return-suspends.js`,
`async-generator-await-using-for-of-head-dispose-tick-alignment.js`, `…-suspended-gc-rooting.js`, plus the extended
`async-generator-await-using-inflight-completion-gc-rooting.js`.
Use the witness-chain + trailing drain pattern (`Promise.resolve().then(w1)…`, then a 12–30 tick drain, then assert) and
a **synchronous** `sync-mid` marker after the `next/throw/return` call — a nested inline drain runs the whole queue
(including the drain's assertion) *inside* the call, which is exactly what makes the blocking bug visible.

Targeted test262 (run before/after, diff against `$TMPDIR/jsse-base`):
`test262/test/built-ins/AsyncGeneratorPrototype/` (esp. `return/`, `throw/`, `next/`),
`test262/test/language/statements/async-generator/`, `test262/test/language/expressions/async-generator/`,
`test262/test/language/expressions/yield/`, `test262/test/language/statements/for-await-of/`,
`test262/test/language/statements/for-of/` (`head-await-using-*`), `test262/test/language/statements/await-using/`,
`test262/test/built-ins/AsyncFromSyncIteratorPrototype/`, `test262/test/language/statements/class/` and
`expressions/class/` async-gen element dirs. Then `uv run python scripts/run-test262.py test262-extra/`,
`uv run python scripts/run-custom-tests.py`, `cargo test --release`, `./scripts/lint.sh` (separately), and the full
`uv run python scripts/run-test262.py` per CLAUDE.md. `tests/` is only needed if a slice adds an engine-internal
observable that is not JS-visible (none expected).

## 7. Regression risk

- **Baseline (`test262-pass.txt`, read from `origin/main`, not to be rewritten):** slice 2 is the one that can move it.
  `return-suspendedYield-*.js`, `return-suspendedStart-*`, `yield-star-*` (tick-count tests such as
  `yield-star-return-then-getter-ticks.js`) and `request-queue-order-state-executing.js` pin tick counts that inline
  drains have been masking. A regression there means the change is wrong somewhere (spec wins over the test only when
  the test is demonstrably mis-specified — then say so in the PR, do not special-case).
- Shared machinery: the queue invariants of ADR-2026-09-21-2300 (a driver-context caller sets
  `async_gen_yield_pending`; a continuation settles and pops the front request exactly once), `async_gen_finish_disposal`
  (L5688), `generator_pending_dispose` GC rooting (`collect_gc_roots`, `free_gc_object`), `route_generator_exception`'s
  idempotence on re-entry, and `take_generator_dispose_stack`.
- Not touched: the tree-walker hot paths (`eval_expr`/`exec_statement`), the property MOP, the bytecode fast path,
  the exhaustive `ObjectKind` matches, sync generators, plain async functions (`async_function_resume`), and the
  Node-compat library harnesses. `GeneratorDisposal` gains no field, so no exhaustive-match churn is expected.
- Merge risk: #714, #720, #729, #731 edit the same file; rebase before slices 3–4 and re-run the targeted dirs after.

## 8. Out of scope (deliberately not bundled)

Items 4 and 6 and the two `for-of`/inline-replay halves of item 2 (§3); the `for await` AsyncIteratorClose Await
(#685/#707); renaming or restructuring `GeneratorDisposeThen`; removing the remaining `drain_microtasks()` after
`call_function(&reject_fn…)` (#712); formatting or unrelated cleanups; any change to `test262-pass.txt`.

## Appendix: probe results (jsse `997eaee1` vs Node 26; tick-order only where noted)

Harness: `var log=[],L=x=>log.push(x); setTimeout(()=>console.log(log.join(',')),60);` prefix; `it.next().then(()=>{L('got1');
<probe>; L('sync-mid')})`.

- s1 `async function*g(){for(await using a of [{[Symbol.asyncDispose](){L('disp')}}]){L('body')}L('after');yield 1}`,
  `it.next().then(settled); L('sync-end')` → jsse `body,disp,after,sync-end,settled`; Node `body,disp,sync-end,after,settled`.
- s3 `try{ {await using a=D; yield 1; return 5} }finally{L('fin')}`, second `next()` → jsse `disp,fin,sync-mid,n2`; Node `sync-mid,disp,fin,n2`.
- s4 `try{yield 1}finally{L('fin')}`, `.return({then(r){L('then');r(7)}})` → jsse `fin,then`; Node `then,fin`.
- s4b `try{yield 1}catch(e){L('caught');yield 'c'}finally{…}`, `.return(Promise.reject('E'))` → jsse `rej:E`; Node `caught:E, ret:c false`.
- s5 `it.return(pending); it.next()` on suspended-start → jsse settles `next` before `return`; Node `return` then `next`.
- s6 `async function*g(){await using a=D; yield* inner()}`, `.return('R')` at `i1` → jsse: `disp` never logged; Node `inner-fin,disp,ret:R`.
- z1 `.throw('T')` at yield in `try{ {await using a=D; yield 1} }catch(e){L('caught')}` → jsse `dispA,caught,sync-mid`; Node `dispA,sync-mid,caught`.
- x1–x3, y1 (throwing disposers replacing an in-flight return / nested frames): values identical to Node; only order vs `sync-mid` differs.
