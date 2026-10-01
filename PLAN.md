# Plan: issue #742 — async-generator for-of unwind / delegated `yield*` disposal gaps

## 0. Important finding: item 1 is already fixed

Before planning, the current branch state (based on `main` past #775/#772/#773) was
audited against the issue's four items. **Item 1 ("remaining
`unwind_generator_for_of_loops` callers still block") is already resolved**, by
PR #761 "refactor(interpreter): deepen async generator unwind continuation",
merged 2026-09-28 — after this issue was filed, as a `pm-deepen` architecture
pass whose own PR body states "tracked in item 1 of #742" and that it ported
the `can_park`/`ForOfUnwindOutcome` primitives #733 built for
`route_generator_exception` to all four remaining callers:

- `route_loop_control_result!` passes `can_park = true`
  (`src/interpreter/eval/generator_runtime.rs:3799`) and parks via
  `park_for_of_unwind` on `ForOfTransitionOutcome::Parked`.
- `align_for_of_result!` (the `Goto` caller) does the same
  (`generator_runtime.rs:3853`/`3874`).
- The `pending_return` block passes `can_park = true`
  (`generator_runtime.rs:3987`) and parks the same way.
- Bare `return;` (`generator_runtime.rs:4622-4625`) routes through that same
  `pending_return` block instead of its own path.
- `return expr;` (`generator_runtime.rs:4572-4618`) now evaluates the operand,
  Awaits it via `async_generator_return_state_machine_with_promise`, and only
  forms the `Return` completion (hence starts for-of unwind) afterward —
  resolving the `Await(exprValue)`-ordering question this issue raised.
- The proposed `generator_pending_for_of_retry` side table was explicitly
  rejected in favor of `GeneratorReentry::{LoopControl, Goto}` carried inside
  the existing `GeneratorDisposal`/`Interpreter::generator_pending_dispose`
  (see `src/interpreter/dispose.rs:205-233`). Do not plan that side table.
- Regression coverage already landed:
  `test262-extra/async-generator-for-of-unwind-resumes-all-actions.js`.

This plan therefore does **not** re-implement item 1. It only updates the two
docs that still describe it as unfixed (CONTEXT.md, ADR-2026-09-22-2340), and
focuses the real work on items 2 and 3. **Item 4** (async *functions'*
`close_for_of_loop` dispose in `eval.rs`) is a different driver with its own
resumable-dispose precedent (`unwind_scopes_to!`/`DisposeThen::ScopeCross*`,
`DisposeThen::ForOfIteration`) and is explicitly deferred to a follow-up issue
— see "Out of scope" below.

## 1. Problem restated

Two gaps remain in the async-generator driver
(`src/interpreter/eval/generator_runtime.rs`). First, several `yield*`
delegation steps that end abruptly (a rejected/malformed inner result, a
`GetMethod` failure fetching the delegate's `return`/`throw`, or a completed
delegated return) settle the `.next()`/`.return()`/`.throw()` request's
promise *directly* instead of raising the completion as an ordinary abrupt
completion inside the generator body — skipping `DisposeResources` entirely
for any `await using` in scope, and skipping the enclosing `try`/`finally`
and any outer `for-of` the `yield*` sits inside. One arm (`ReturnAwait`) also
Awaits the delegated return value in the wrong position relative to disposal.
Second, the frame-leave disposal loop that disposes `await using` block
scopes a state transition leaves forces the *blocking* dispose path whenever
`is_inline_replay` is true, with no resumable equivalent — and, independent
of the original premise (the `InlineYield` fallback never being retired),
there is a latent flag-staleness bug where a throw/return routed to a
different state can inherit a stale `is_inline_replay = true` from an
unrelated earlier resume.

## 2. Spec basis

- **`YieldExpression : yield * AssignmentExpression`**
  (`sec-generator-function-definitions-runtime-semantics-evaluation`,
  `spec/spec.html:24279-24334`). Every step that can fail —
  `? Call(nextMethod, …)`, `? Await(innerResult)`, "If innerResult is not an
  Object, throw a TypeError exception", `? IteratorComplete(innerResult)`,
  `? IteratorValue(innerResult)`, `? GetMethod(iterator, "throw"/"return")` —
  is an **abrupt completion of the `YieldExpression` evaluation itself**. It
  must propagate exactly like any other abrupt completion produced by
  evaluating an expression inside the generator body: through enclosing
  `try`/`catch`/`finally`, through any `for-of` the `yield*` is lexically
  inside, and only then into the function's own `DisposeResources`. Settling
  the async-generator request directly, bypassing the body, is not what the
  grammar describes.
- **No-`return`-method branch** (`spec/spec.html:24319-24324`): `receivedValue
  = received.[[Value]]`; if the generator is async, `Await(receivedValue)`;
  `Return ReturnCompletion(receivedValue)`. The Await happens *before* the
  `ReturnCompletion` is formed, i.e. before any unwind/dispose starts — the
  same ordering #761 already gave `return expr;` via
  `async_generator_return_state_machine_with_promise`.
- **Inner-`return`-done branch** (`spec/spec.html:24325-24331`): `Call(return,
  iterator, …)` → `Await(innerReturnResult)` → `IteratorComplete` →
  `IteratorValue` → `Return ReturnCompletion(returnedValue)`. The value is
  already fully Awaited by the time `ReturnCompletion` is formed; nothing
  Awaits it again.
- **`AsyncGeneratorYield`** (`sec-asyncgeneratoryield`, `spec/spec.html:50789
  -50821`) always finishes via `? AsyncGeneratorUnwrapYieldResumption`
  (`spec/spec.html:50772-50787`), which Awaits a return completion's value
  once before it ever reaches the `yield*` loop's `received` variable. So the
  no-`return`-method branch's own `Await(receivedValue)` above is a **second**
  Await of an already-resolved value — genuinely one more tick, not a no-op —
  and must not be elided.
- **`AsyncGeneratorStart`** (`sec-asyncgeneratorstart`, `spec/spec.html:50645
  -50679`), step "If `result` is a return completion, set `result` to
  `NormalCompletion(result.[[Value]])`" (step 50670): no further Await of the
  final body-level return value happens here. Confirms the value produced by
  the two branches above is used as-is once it reaches the top.
- **`ReturnStatement : return Expression ;`** (`sec-return-statement-runtime-
  semantics-evaluation`, `spec/spec.html:22745-22758`) is the clause that
  Awaits a literal `return expr;`'s operand — unrelated to, and not reused
  by, `yield*`'s own internal `ReturnCompletion(...)` steps, so the item-3 fix
  does not touch `return expr;` handling (already correct, item 1).
- `DisposeResources`/`SuppressedError` chaining: sourced from the Explicit
  Resource Management proposal, not yet merged into the pinned `spec/`
  snapshot — treated as authoritative per repo convention, consistent with
  ADR-2026-09-21-2015/2026-09-22-2326/2026-09-22-2340.
- Item 2 (inline-yield-replay disposal) changes no observable JavaScript
  semantics by itself — `SentValueBindingKind::InlineYield` is an *engine*
  fallback mechanism (CONTEXT.md "Inline Yield"), not spec vocabulary. Its
  governing spec basis is the same `DisposeResources`/Await-ordering clauses
  already cited for item 1; the fix (if any) is "resume correctly," not a
  new behavior.

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the delegated-`yield*`
  abrupt-exit arms (item 3) and the frame-leave `is_inline_replay` probe/fix
  (item 2).
- `src/interpreter/dispose.rs` — remove `GeneratorDisposeThen::ReturnAwait`
  and its handling arm in `async_gen_finish_disposal`, and `AwaitReturnStart`
  if it becomes dead too, once item 3's slice 4 removes their last callers.
- `CONTEXT.md` — fix the "Await Using Block Scope" entry's stale sentence
  ("...still blocks on its other four callers ... and on inline-yield
  replay") now that #761 fixed the four callers; update once item 2's
  outcome is known.
- `docs/adr/2026-09-22-2340-async-generator-for-of-unwind-suspends.md` —
  amend "Known boundaries" with an "Update (#761)" note (mirroring how
  ADR-2026-09-22-2326 already records the #733 update), since #761 did not
  touch this file.
- `docs/adr/2026-10-01-<time>-async-generator-yield-star-abrupt-exit-disposal.md`
  (new) — records the item-3 decision: every `yield*` abrupt-exit arm
  delivers its completion into the body via `deleg_info.resume_state`
  instead of settling the request directly, and the no-`return`-method
  branch Awaits *before* forming the completion. States explicitly that it
  supersedes ADR-2026-09-21-2246/2026-09-22-2326's `ReturnAwait`
  characterization ("awaited after disposal") for the `yield*`-return case.
- `test262-extra/` — new regression files per slice below (exact names
  chosen by the implementation stage to match existing conventions, e.g.
  `async-generator-yield-star-*`).

## 4. TDD slices

0. **Docs-only, no test.** Fix CONTEXT.md's stale sentence and amend
   ADR-2026-09-22-2340's "Known boundaries" with the #761 update, so the
   architecture docs stop contradicting the code before any further change
   lands on top of them.

### Item 3 — delegated `yield*` abrupt exits

1. **Generalize the `has_catch` special case.** In
   `yield_star_await_inner_result_resume`'s `IteratorValue` error arm
   (`generator_runtime.rs:3058-3107`), delete the `has_catch` branch
   entirely and always deliver the error into the body: snapshot
   `IteratorState::StateMachineAsyncGenerator` at
   `SuspendedAtState { state_id: deleg_info.resume_state }` with
   `delegated_iterator: None`, then call `self.async_gen_reenter(gen_id,
   Completion::Throw(e), request)` (this is the generic helper the `has_catch`
   arm already half-built; factor it into a small private method shared by
   the rest of this slice list, since `async_gen_reenter` itself writes
   `pending_exception`/`pending_return` and re-invokes the driver).
   - Red test: `test262-extra/async-generator-yield-star-reject-propagates-
     through-finally.js` — `try { yield* { ... IteratorValue getter throws
     ... } } finally { log.push('f'); }` with **no** `catch`; assert `'f'`
     ran before the request rejects (today: rejects directly, finally never
     runs).
   - Keep a `try { yield* ... } catch (e) { ... }` variant to confirm the
     already-passing `has_catch` behavior survives the generalization.
2. **Apply the same helper to the sibling reject arms** in
   `yield_star_await_inner_result_resume`: the rejected `Await(innerResult)`
   arm, the "not an Object" arm, and the `IteratorComplete` error arm
   (currently all `self.retire_generator(gen_id); reject_fn(...)` directly).
   - Red test: extend the slice-1 file (or a sibling file) with delegates
     whose `next()`/`throw()` resolves to a rejected thenable, a non-object,
     or an object whose `done` getter throws — each inside
     `try { yield* X; } finally { log.push('f'); }`.
3. **Fix the `GetMethod` failure arm** in `yield_star_return_after_unwrap`
   (`self.iterator_return(&iterator, &awaited_val)`'s `Err(e)` arm, currently
   ~`generator_runtime.rs:3375-3382`) the same way: deliver as
   `pending_exception` at `deleg_info.resume_state` instead of rejecting
   directly.
   - Red test: a delegate whose `return` accessor throws when read, while a
     `.return()` call is in flight during delegation, inside `try { } finally
     { }`.
4. **Fix the return-completion Await ordering** (the core semantic bug):
   - `yield_star_complete_with_return`'s caller already has an
     already-Awaited `value` (either from `AsyncGeneratorYield`'s own
     `AsyncGeneratorUnwrapYieldResumption`, or from `Await(innerReturnResult)`
     at the inner-`return`-done step). Stop routing it through
     `GeneratorDisposeThen::ReturnAwait`; deliver it directly as
     `pending_return` via the slice-1 helper and let the ordinary resumable
     `pending_return` path (item 1) dispose it during unwind.
   - The two "no `.return()` method" arms (driver's delegation prelude
     `Ok(None)` arm, and `yield_star_return_after_unwrap`'s `Ok(None)` arm)
     currently hand a **raw, un-Awaited** operand to the same
     `ReturnAwait` path. Fix them to `await_then` the raw value first
     (mirroring `async_generator_return_state_machine_with_promise`'s
     existing pattern exactly — do not skip this second Await; see spec
     basis above), then deliver the awaited value as `pending_return` the
     same way.
   - Delete `GeneratorDisposeThen::ReturnAwait` (`dispose.rs:229`) and its
     arm in `async_gen_finish_disposal` (`generator_runtime.rs:5728-5734`),
     and `AwaitReturnStart`/`async_gen_await_return` if nothing else calls
     them, once all three call sites are converted (clippy `-D warnings`
     will catch anything left dangling).
   - Red test: `try { yield* X; } finally { log.push('f'); }` for both the
     inner-done-return and no-return-method shapes, asserting `'f'` runs
     before the request settles. Also a tick-counting witness-chain test
     (ADR-2026-09-22-2340's style: pre-queued `Promise.resolve().then()`
     reactions) distinguishing the no-return-method case's *two* Awaits from
     the inner-done case's *one*.
   - Regression check (not a new test): re-run
     `test262-extra/async-generator-yield-star-return-disposes-function-
     level-resource.js` unmodified — its assertions are ordering-only, not
     tick-count-sensitive, so they should stay green through this change,
     but confirm.
5. **Outer `for-of` closing.** Add a regression test with the `yield*` nested
   inside `for (await using x of outerIter) { yield* X; }` (no `try` at all),
   asserting `outerIter`'s `.return()` runs when the delegation ends
   abruptly (today: skipped, since the old code path never reaches the
   state machine's normal unwind). This should pass once slices 1-4 route
   through `check_abrupt_on_resume` → `route_exception!`/the `pending_return`
   block, which item 1 already made resumable for for-of — no additional
   production code expected, but it is the issue's own explicit "outer
   for-of closing" claim and must be locked in by a test, not assumed.
6. **No-`.throw()`-method arm** (`generator_runtime.rs:3547-3557`). Per spec
   (`spec/spec.html:24310-24316`) this must perform `AsyncIteratorClose`
   (calling the delegate's `return`, if present, and Awaiting its result)
   *before* throwing the `TypeError` into the body — a second asynchronous
   step beyond slices 1-5's shape. Attempt it last with its own `await_then`
   continuation that then delivers via the slice-1 helper. **If this turns
   out to need materially more plumbing than slices 1-5, stop here and leave
   it as a named follow-up** (see "Out of scope") rather than let it block
   the rest of the PR — the issue itself flags this as the hardest arm.

### Item 2 — inline-yield-replay disposal

7. **Probe first, fix only if red.** `is_inline_replay`'s forced
   `can_park = false` (`generator_runtime.rs:4101`) was originally framed as
   blocked on #625 (closed without a deliberate retire-or-keep decision, but
   the `InlineYield` fallback is still live code — see open #771). Rather
   than fixing the flag's generic unresumability (which may be dead in
   practice: a plain `.next()` replay re-enters the *same* state, so
   `scope_depth` does not change and the frame-leave loop never fires), probe
   the specific reachable hazard: `pending_binding`'s `InlineYield` payload is
   applied unconditionally on driver entry (`generator_runtime.rs:3642-3668`)
   even when `stored_pending_exception`/`stored_pending_return` is also set,
   but `inline_yield_target` is only `.take()`n *after* the
   `check_abrupt_on_resume` block's routing `continue` — so a `.throw()`/
   `.return()` that gets routed to a *different* state than the one the
   generator was suspended at can resume with a stale
   `is_inline_replay = true` and a stale `self.generator_context`
   attributed to the wrong state.
   - Write a regression test: suspend an async generator via the
     `InlineYield` fallback (e.g. a `yield` inside a destructuring default,
     per CONTEXT.md's "Inline Yield" entry) inside a `try`, with an
     `await using` resource scoped to the `catch`/`finally` target, then
     call `.throw()` with a handler present. Assert (witness-chain style)
     that disposal at the handler's scope suspends correctly and that
     `self.generator_context` is not misapplied to the handler's state (no
     spurious re-yield / wrong fast-forward count / hang).
   - **If red:** fix by clearing `inline_yield_target` (and not re-deriving
     it from a stale `pending_binding`) inside the `check_abrupt_on_resume`
     routing paths before any `continue`, so a routed completion never
     inherits an unrelated state's replay marker. Keep the fix to that one
     staleness clear — do not also attempt full resumability for genuine
     same-state inline-replay disposal in this slice.
   - **If green:** the scenario is unreachable in practice (mirrors #625's
     unresolved-but-moot finding). Keep the test as locked-in regression
     coverage and record "probed, unreachable, no production change" in the
     PR description — do not force a speculative fix. Update CONTEXT.md's
     "still blocks... on inline-yield replay" clause accordingly either way.

## 5. Test surface

- `uv run python scripts/run-test262.py test262/test/built-ins/AsyncGeneratorPrototype/`
  (covers `next`/`return`/`throw`).
- `uv run python scripts/run-test262.py test262/test/language/statements/async-generator/`
  and `test262/test/language/expressions/async-generator/`.
- Targeted `yield*` sweep: every file matched by
  `find test262/test -iname '*yield-star*'` (185 files as of this plan,
  spanning both sync- and async-generator `yield*`); these pass today via
  the direct-reject path for the no-handler/no-resource case and must
  continue to reject in the same tick once routed through the body (an empty
  dispose stack / no handler still settles synchronously — verify, don't
  assume).
- `test262/test/language/statements/using/` and
  `test262/test/language/statements/for-await-of/` plus any async-generator
  `for-of`/`await using` interaction tests, since slice 5 touches outer
  for-of closing.
- Full suite before calling the PR done: `uv run python scripts/run-test262.py`.
- `uv run python scripts/run-custom-tests.py`.
- `cargo test --release` (covers `dispose.rs`'s unit tests — update/remove
  any that reference `ReturnAwait` directly).
- `./scripts/lint.sh` (clippy `-D warnings` will fail on a dead
  `ReturnAwait`/`AwaitReturnStart` left behind after slice 4).
- New spec-correct behavior not covered by test262 (the engine-internal
  `GeneratorDisposeThen`/`is_inline_replay` staleness probe, and the exact
  Await-tick-count distinction between the two return-completion shapes)
  goes in `test262-extra/`, following the existing file pattern (`esid`,
  `info` quoting the exact spec steps, `features: [explicit-resource-
  management, async-iteration]` where relevant).

## 6. Regression risk

- All changes are inside `async_generator_next_state_machine_impl` and its
  delegation helpers, plus `dispose.rs`'s `GeneratorDisposeThen`. This is
  hot, shared machinery: every async generator using `yield*`, `await using`,
  or `for-of` goes through it, so a mistake here is baseline-moving, not
  isolated.
- `async_gen_reenter`/`async_gen_reenter_with_reentry` is also used by the
  already-correct direct-`.return()`-at-a-yield path and item 1's for-of
  unwind; slices 1-5 add callers but do not change its own logic — verify
  that holds (no edits to `async_gen_reenter` itself should be needed).
- GC rooting: any new `await_then` closure (slice 4's no-return-method Await,
  slice 6's AsyncIteratorClose Await) must root captured values the same way
  `async_generator_return_state_machine_with_promise` already does
  (`with_gc_root_scope`/`gc_root_value` before scheduling); a missed root is
  a use-after-collection, not a test262 regression that shows up locally.
- Deleting `GeneratorDisposeThen::ReturnAwait` is a breaking change to an
  internal enum — confirm via `cargo build --release` plus clippy that no
  other match arm (including in `dispose.rs`'s own `#[cfg(test)]` module, if
  any references it) still expects that variant to exist.
- Bytecode fast path (`bytecode_enabled`, off by default): generator/async
  state-machine bodies do not pass through `dispatch_body` per CLAUDE.md, so
  this change has no bytecode interaction — no bytecode-specific testing
  needed, but note it so a reviewer doesn't go looking.
- Node-compat library harnesses (`zod`, `moment`, etc.) exercise async
  generators indirectly at most; not expected to move, but
  `./scripts/run-library-tests.sh` is cheap insurance if any of them use
  `for await`/`yield*` internally — check before skipping.
- `test262-pass.txt` baseline: do not update it (read-only from
  `origin/main` per repo convention); report the before/after pass counts
  from `run-test262.py` in the PR description instead.

## 7. Out of scope

- **Item 4** (async functions' `close_for_of_loop` dispose in `eval.rs`,
  `unwind_for_of!` macro) — a different driver with its own resumable-dispose
  precedent already in place (`unwind_scopes_to!`/`DisposeThen::ScopeCross*`
  for block scopes, `DisposeThen::ForOfIteration` for the per-iteration head
  dispose, both in `src/interpreter/dispose.rs`/`src/interpreter/eval.rs`).
  The fix shape mirrors ADR-2026-09-22-2340's generator-side pattern exactly
  (keep the closing loop on `for_of_stack` with `iteration_env` taken, park
  via `async_fn_suspend_at_await`/`park_async_function_dispose` with a new
  `DisposeThen::ForOfCross*` variant, resume into the same unwind loop). File
  as its own follow-up issue rather than bundling two independent drivers'
  resumability fixes into one PR.
- **Slice 6** (no-`.throw()`-method arm's `AsyncIteratorClose` Await), if it
  proves more involved than slices 1-5 — file as a follow-up issue naming the
  exact arm and spec steps (`spec/spec.html:24310-24316`) rather than rush it.
- Any refactor of `async_gen_reenter`/`async_gen_reenter_with_reentry`
  themselves, or of the `GeneratorReentry`/`GeneratorDisposal` types #761
  just finished generalizing — this plan only adds callers.
- Formatting or unrelated cleanup elsewhere in `generator_runtime.rs` (it is
  a 6600+ line file with plenty of unrelated surface area).
- Rolling `test262-pass.txt` forward (a `main`-branch-only operation).
