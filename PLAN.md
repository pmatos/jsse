# Plan: issue #665 — third slice: abrupt exits (`break`/`continue`/`return`/`throw`) from `for (await using x of …)` in async functions

## 0. Where #665 stands (read first — this replaces the earlier plan)

- Landed on `main`: #666 (function-level + `disposeAsync`), #688 (nested `await using` blocks in try/loop/switch
  bodies), #701 (real block scope states, closes #683), #703 (per-entry scope for lowered blocks/loops/catch,
  closes #684), #699 (`for (await using x of …)` head parsed as sync iteration + per-iteration disposal parked at
  `ForOfHead`). The previous `PLAN.md` (parse bug + `ForOfHead` parking) is **fully merged; do not redo it**.
- This branch was reset to `origin/main` (`a129a725`) by the planner (`backup/665-pre-rebase` holds the old tip).
  The remote branch was deleted after #699 merged (`git ls-remote origin <branch>` → empty), so a plain
  `git push -u origin HEAD` works; if the ref reappears, `git fetch origin <branch>` then `--force-with-lease`.
- Still open trackers: #685 (loop heads + iterator close), #686 (async generators), #687 (other blocking
  `await_value` callers). This PR is `Refs #665`, `Refs #685` — **not** `Closes`.

Probe on `a129a725` (witness chain `w1..w8` started before the call, `L` logs, `sync-end` logged after the call;
script pattern: `test262-extra/await-using-try-catch-finally-dispose-tick-alignment.js` `observe(shape)`):

| shape | node | jsse (main) | cluster |
|---|---|---|---|
| `for (await using a of [null,null]) { L('b'); break }` `L('after')` | `b,sync-end,w1,after,w2,settled` | `b,w1,after,sync-end,w2,settled` | **this PR** |
| same with `return 1` | `b,sync-end,w1,w2,settled` | `b,w1,sync-end,w2,settled` | **this PR** |
| body `throw 1` caught by outer `try` | `b,sync-end,w1,caught,after,w2,settled` | `b,w1,caught,after,sync-end,w2,settled` | **this PR** |
| custom iterator with `return()` logging `ret`, then `break` | `b,sync-end,w1,ret,after,w2,settled` | `b,w1,ret,after,sync-end,w2,settled` | **this PR** |
| `for (await using a = null; false;) {}` (C-style head) | `sync-end,w1,after,w2,settled` | `w1,after,sync-end,w2,settled` | follow-up B |
| async-generator block / `for-of` head / `return()` | see #686 | blocks inline | follow-up C |
| `for await (var a of [1,2]) { break }` (no `await using` at all) | `sync-end,w1,w2,b,w3,after,…` | `…b,after,…` | follow-up D |

## 1. Problem restated

For a lowered `for (await using x of iterable) body` in an async function or TLA module, a **normal** iteration end
already suspends at the disposal `Await` (#699). But when the body *exits the loop abruptly* — `break`, labeled
`continue` to an outer loop, `return`, or an uncaught/propagating throw — the loop is closed by
`close_for_of_loop` (`src/interpreter/eval.rs:~9661`), which runs the iteration environment's DisposeResources through
the blocking driver (`dispose_resources` → `run_dispose_cursor_blocking`, draining the microtask queue inline at each
`Await`). Queued jobs therefore run in the middle of synchronous code and the function settles on the wrong tick.
Two entry points reach it: the `unwind_for_of!` macro (`~8317`, used by `route_return!` and `route_loop_control!`
and the direct `unwind_for_of!(pos)` at `~9006`) and `unwind_async_for_of_loops` (`~9709`, the throw path at `~8835`).

## 2. Spec basis

`spec/` (ecma262) predates Explicit Resource Management: it has no `await using` `ForDeclaration` and no
DisposeResources. Base clauses that exist:

- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`** (`spec/spec.html:22388`),
  the tail of the loop (`:22453-22461`): `result` = evaluation of `stmt`; "If LoopContinues(result, labelSet) is
  false: `status` = UpdateEmpty(result, V); … `IteratorClose(iteratorRecord, status)`". The
  [proposal-explicit-resource-management] text inserts `DisposeResources(iterationEnv.[[DisposeCapability]], result)`
  **between** evaluating `stmt` and the `LoopContinues` test — so an abrupt body completion first disposes
  (awaiting for `await using`), and only then is the iterator closed with the resulting completion (a disposer
  throw replaces `break`/`return` and is then the `status` passed to `IteratorClose`).
- **`sec-iteratorclose`** (`:7162`): a throw `completion` wins over any error from `return()`; otherwise a `return()`
  failure replaces `break`/`continue`/`return`. Already implemented by `iterator_close_result` + `close_for_of_loop`;
  unchanged.
- **`sec-disposeresources`** (proposal; reproduced in the `info:` blocks of the existing
  `test262-extra/await-using-*.js` and test262's `language/statements/await-using/`): step 3.f (`needsAwait`),
  step 4 (trailing `Await(undefined)`), `Dispose` step 3 (`Await(result)`); implemented by `DisposeCursor`
  (`src/interpreter/dispose.rs`), unchanged.
- **`await`**: the disposal `Await` suspends the running async-function context; the continuation is a later job.

test262 already in tree that pins adjacent behavior (must stay green):
`language/statements/for-of/head-await-using-*.js`, `language/statements/for-await-of/head-await-using-init.js`,
`language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-each-iteration-of-forofstatement.js`.
test262 has no abrupt-exit tick-ordering test → new `test262-extra/` files (§5).

## 3. Design (implementer verifies point 1 first — it decides whether the slice is cheap)

The parked-cursor machinery is generic (`PendingDispose { cursor, then: DisposeThen }`,
`Scheduler::park_async_function_dispose`, top-of-loop cursor stepper in `async_function_resume`). `unwind_scopes_to!`
already parks in the middle of an unwind and **re-enters** `route_return!` / `route_loop_control!` / the throw routing
from scratch on resume, relying on the recompute being idempotent. Apply the same trick to for-of loops:

1. **Do not pop the loop before disposing.** In the unwind, look at `for_of_stack.last_mut()`: `iteration_env.take()`,
   `take_dispose_stack(&env)`. If the stack has only `Sync`-hint resources, finish inline (no `Await` is possible,
   same short-cut #699 used at `ForOfHead`). Otherwise build `DisposeCursor::new(stack, seed)` (seed = the completion
   in flight: `Return(v)` for return routing, `Empty` for loop control, `Throw(exc)` for throw routing) and `step` it:
   `Await` → GC-root frame, `async_fn_suspend_at_await(...)` with the **loop still on `for_of_stack`** (it is saved
   in the async state), `park_async_function_dispose(id, PendingDispose { cursor, then })`, return. `Done(Exit)` →
   `remove_async_function_state` + `return Exit`. `Done(Throw(e))` → continue as a disposer throw (below).
2. **New `DisposeThen` variants**, one per caller, mirroring `ScopeCrossReturn/LoopControl/Throw`:
   `ForOfCloseReturn`, `ForOfCloseLoopControl(LoopControlTarget)`, `ForOfCloseThrow`. On resume with a **non-throw**
   completion, re-enter the caller (`route_return!(v)` / `route_loop_control!(target)`); the top loop now has
   `iteration_env == None`, so the unwind skips its dispose half, runs **only** the iterator-close half, pops it and
   proceeds to the next loop. On resume with `Throw(e)` set `pending_for_of_unwind = Some(PendingForOfUnwind {
   clear_at_state: None })` and `pending_exception = Some(e)` — exactly what `unwind_for_of!`'s existing
   `Completion::Throw` tail does — so the throw routing closes the remaining loops (this one's `return()` still runs,
   with the throw as its completion, errors from `return()` suppressed).
3. **Split `close_for_of_loop` into two halves** in `eval.rs`: the dispose half (env → `DisposeCursor`) and
   `close_for_of_iterator(loop_state, func_env, completion, generator_id)` = the existing body from
   "The borrow must end before `iterator_close_result`…" onward. Keep `close_for_of_loop` as a thin composition of the
   two using the **blocking** dispose driver — it is also called by `generator_runtime.rs:~6804`
   (`Some(generator_id)`, sync/async-generator unwinding) which must stay blocking in this PR.
4. **The throw path** (`unwind_async_for_of_loops`, call at `~8835`) is not the macro. Give the throw routing its own
   parked variant of the same loop (same seam: dispose half may park with `ForOfCloseThrow`; on resume the routing
   block is re-entered with `pending_exception = Some(e)`, and `needs_for_of_unwind` is recomputed from the still
   non-empty `for_of_stack`). The blocking `unwind_async_for_of_loops` helper can then be deleted if unreferenced
   (otherwise the fmt/clippy hook fails on dead code) — check `generator_runtime.rs` for other users first.
5. Suspension inside `unwind_for_of!` must persist `pending_return` / `pending_loop_control` /
   `saved_finally_exception` / `pending_for_of_unwind` through `async_fn_suspend_at_await` exactly as
   `unwind_scopes_to!` does (copy its argument list; a `.take()` there is intentional).
6. **Ordering to preserve** with a finalizer between the loop and its handler: `route_return!` computes `unwind_from`
   from the intercepting `finally` (`routed_to`), so only loops nested inside it are closed now. Re-entry recomputes
   the same value; do not cache it across the suspension.

Note the direct `unwind_for_of!(pos)` at `~9006`: it is the *unlabeled-break* fallback (`Completion::Break(None, _)` out
of a state body when no `block_exits` target matched); it closes to `pos` then jumps to `after_state`. It does not go
through `route_*`, so its resume arm must set `current_id = for_of_stack[pos].after_state` (capture it before parking)
— add a `DisposeThen` variant or route it through `route_loop_control!` if that is equivalent. If re-entry is **not**
idempotent for some macro, stop, keep that call site blocking, and record it in the PR body and #685 rather than
growing a bespoke continuation.

## 4. Files to touch

- `src/interpreter/dispose.rs` — three `DisposeThen` variants (doc comment each, matching existing style).
- `src/interpreter/eval.rs` (`async_function_resume` + helpers) — the macro `unwind_for_of!` (dispose-then-park),
  the throw-routing call to `unwind_async_for_of_loops`, the top-of-loop `DisposeStep::Done` match (3 new arms, keep
  the `Completion::Exit` arm ahead of them — issue #242), split of `close_for_of_loop`.
- `src/interpreter/eval/generator_runtime.rs` — **no behavior change**; only its `close_for_of_loop` call site is
  re-verified against the split (still blocking).
- `src/interpreter/gc.rs` — none expected: parked cursors are already traced through `AsyncFunctionState.pending_dispose`
  (`gc.rs:~471`) and the loop states through the saved `for_of_stack`. Covered by a regression test rather than a change.
- `CONTEXT.md` — one glossary line only if a new term is introduced; no ADR.
- Tests: §5.

Not touched: `exec.rs` (tree-walker loops), `scheduler.rs` (parking API already generic), the parser, the transform
(`generator_transform.rs` — loops are already lowered by #699), `test262-pass.txt`, `spec/`, `test262/`.

## 5. TDD slices (red → green; one commit each; conventional-commit subjects; `Refs #665`)

Before slice 1, per the global instruction, turn this list into TaskCreate tasks with `addBlockedBy` ordering. Build with
`cargo build --release -j4`; run gates as separate commands (never `&&`-chained); run `test262-extra/` via
`uv run python scripts/run-test262.py test262-extra/<file>`. Compute every expected trace with `node` first, then
write it into the test as the literal expectation.

1. **`test(disposable): pin abrupt for-of exits from await using heads (red)`** —
   `test262-extra/await-using-for-of-close-dispose-tick-alignment.js` (`esid:
   sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`, `info:` quoting the
   `LoopContinues`/`IteratorClose` tail + DisposeResources 3.f/4, `flags: [async]`, `includes: [asyncHelpers.js,
   compareArray.js]`, `features: [explicit-resource-management]`, same `observe(shape)` witness harness). Shapes: `break`
   (null resource), `return`, body `throw` caught by an outer `try`, custom iterator whose `return()` logs (asserts
   dispose tick **before** `ret`), async disposer (asserts `disp` before `ret`), labeled `continue outer` crossing an
   inner `for (await using …)`, two nested `for (await using …)` loops left by one `return`, `break` inside a
   `try/finally` inside the loop, a disposer that throws on `break` (error propagates, `return()` still called, later
   code not run), a disposer returning a rejecting promise. Commit only once red is observed (mark in the commit body).
2. **`refactor(async): split close_for_of_loop into dispose and iterator-close halves`** — no behavior change; existing
   suites green; generator caller unchanged. (Keeps slice 3 small and lets clippy see no dead code: land the helper
   together with its use if the hook blocks.)
3. **`fix(disposable): suspend at for-of iteration disposal on break/continue/return`** — `DisposeThen::ForOfCloseReturn`
   / `ForOfCloseLoopControl` + the `unwind_for_of!` macro change + resume arms. Turns the break/return/iterclose/
   labeled-continue/nested cases green.
4. **`fix(disposable): suspend at for-of iteration disposal on the throw path`** — `ForOfCloseThrow` + the
   throw-routing loop; turns the throw/rejecting-disposer/suppressed cases green.
5. **`test(disposable): GC-root parked for-of close disposal`** —
   `test262-extra/await-using-for-of-close-dispose-suspended-gc-rooting.js`, next to
   `await-using-for-of-head-dispose-suspended-gc-rooting.js`: the disposer allocates and forces GC while parked
   mid-`break`, then the iterator's `return()` must still run on the (still-rooted) iterator.
6. **`test(disposable): pin module top-level abrupt for-of exit ticks`** —
   `test262-extra/await-using-module-for-of-close-dispose-tick-alignment.js` + `_FIXTURE.mjs`
   (`flags: [module, async]`), same layout as `await-using-module-for-of-head-dispose-tick-alignment*`.
   Should be green with no code change (module bodies use `async_function_resume`); if not, fix the call-site gap here.

## 6. Test surface

Targeted test262 (run each; no regressions vs `origin/main:test262-pass.txt`):
`test262/test/language/statements/for-of/`, `.../for-await-of/`, `.../await-using/`, `.../using/`,
`.../for/`, `.../async-function/`, `.../async-generator/`, `.../try/`, `.../labeled/`,
`test262/test/language/expressions/await/`, `.../module-code/top-level-await/`,
`test262/test/built-ins/DisposableStack/`, `.../AsyncDisposableStack/`. Then the full default
`uv run python scripts/run-test262.py` (never rebuild the binary while it runs). Also
`uv run python scripts/run-test262.py test262-extra/` (must stay 100% green), `cargo test` (lib + bin),
`uv run python scripts/run-custom-tests.py`, `./scripts/lint.sh`.

Not covered by test262 (hence the `test262-extra/` files in §5): tick alignment of abrupt-exit disposal, dispose-before-
`return()` ordering, GC rooting while parked mid-unwind, module variant. Regression probe (not committed): the §0
table plus `for_body`/`while_body`/`try_block` shapes, which must stay equal to node.

## 7. Regression risk

- **Hot path:** `async_function_resume` (`eval.rs`) — the unwind macros are shared by every lowered async function
  with a for-of. Guard: the park path is taken only when the iteration env has a dispose stack containing an
  `Async`-hint resource; plain `for (const x of …)` loops see `take_dispose_stack` → `None` and follow today's path
  unchanged (assert with the existing for-of/for-await-of suites).
- **Macro re-entry idempotence** (§3.1) is the main risk: `route_return!`/`route_loop_control!` recompute
  `routed_to`, `unwind_from`, `scope_target` on every entry. A loop left with `iteration_env == None` must be skipped by
  the dispose half but still iterator-closed exactly once (double `return()` call is the failure mode to test).
- **Iterator-close correctness:** a disposer throw during `break` must make `IteratorClose` see a throw completion
  (errors from `return()` suppressed); a `return()` failure after a *successful* disposal replaces the break. Both are
  in the slice-1 shapes.
- **GC:** the loop stays on `for_of_stack` during suspension (saved state) and the cursor is traced via
  `pending_dispose`; the new GC test proves neither drops the iterator.
- **Bytecode fast path:** compiler bails on `statement:ForOf`; no change. **Property MOP / `ObjectKind` matches:** untouched.
- **Baseline:** expect no `test262-pass.txt` movement; do not touch it (runner diffs against `origin/main`).
- **Node-compat library harnesses:** none of the wired libraries use `await using`; no run needed beyond `cargo test`.

## 8. Out of scope (follow-ups; file/append to the trackers named)

- **A. #687 / generator-runtime:** `close_for_of_loop` with `Some(generator_id)`; the other blocking `await_value` callers
  (`eval.rs:936,1022`, `exec.rs:2310`, `generator_runtime.rs:3120,3343,3526,3740,4307,5609,6144,6182`). The tree-walker
  cannot suspend; the fix for those is lowering more shapes, not parking.
- **B. C-style `for (await using a = …; …; …)` heads (#685):** tree-walked in `exec.rs:~1866/1933`, so
  `for (await using a = null; false;) {}` drains inline. Probed: when such a `for` *is* lowered because its body has
  another suspension, `transform_for_statement` gives an `await using` head **no scope** (`per_iteration_bindings`
  covers only `let`/`const`), so the head disposes at *function* end instead of loop end
  (`for (await using a = d;;) { await 0; … } L('after')` → jsse `…,after,disp` vs node `…,disp,after`). Fix shape:
  wrap the loop in `transform_scope_block`-style `EnterScope`/`ExitScope`, break target after the `ExitScope`. Not
  "predicate widening only" — separate PR.
- **C. #686 async generators:** `EnterScope`/`ExitScope` are `unreachable!` in the async-generator executor
  (`generator_runtime.rs:~5940`) and there is no `pending_dispose` on `IteratorState::StateMachineAsyncGenerator`;
  ~13 `dispose_resources` sites in `async_generator_next_state_machine_impl` and the return/throw entry points each need
  a continuation that resolves/rejects the request promise. New state → own PR. (`agen_return` in the probe only *looks*
  hung because the inline drain outruns the 20-tick harness; run standalone it completes.)
- **D. `AsyncIteratorClose` missing `Await(return())` result** for `for await … break` in lowered async functions
  (`sec-asynciteratorclose`; diverges from node with **no** `await using` present, see the table). Different mechanism
  (`iterator_close_result`), not a disposal bug — do not fix opportunistically here; file it.
- **E. Nested-container lowering gap:** a `for (await using …)`/`for await` head nested in `if`/`try`/loop body with
  nothing else suspending falls back to the tree-walker (noted on #699/#685).
- No formatting/cleanup outside touched lines; no ADR; no `test262-pass.txt` update.

## 9. PR / issue hygiene

- Title: `fix(disposable): suspend at for-of disposal on abrupt loop exits (break/continue/return/throw)`.
  Body: `Refs #665`, `Refs #685`; the node-vs-jsse table; list §8 A–E as remaining. Do **not** use `Closes`.
- After merge, comment on #665/#685 with the remaining list (§8) and file D as a new `needs-triage` issue.
- The implementation stage `git rm`s this `PLAN.md` before opening the PR.
