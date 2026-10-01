# Plan: issue #770 — outer `finally` runs twice when an `await` inside a nested try/catch body rejects

## 1. Problem restated

In an `async function`, when a `try { ... } finally { ... }` with no `catch`
of its own directly encloses a `try { ... } catch (e) { ... }` with no
`finally` of its own, and the inner `catch` body contains an `await` whose
promise rejects, the outer `finally` block runs **twice** instead of once
before the rejection propagates out of the function. The minimal repro from
the issue:

```js
async function f() {
  try {
    try {
      throw {};
    } catch (e) {
      await Promise.reject(new Error('rejected-in-body'));
    }
  } finally {
    log.push('finally');
  }
}
```

jsse (confirmed on current branch, built from `b887b68b`) logs
`["finally","finally","rejected:rejected-in-body"]`; node logs
`["finally","rejected:rejected-in-body"]`. Confirmed by direct repro
(`./target/release/jsse -e "..."`) during planning. The same nested shape
with a sync generator's `.throw()` and with an async generator's rejecting
`await` both already produce the correct single `"finally"` — this bug is
isolated to the plain-`async function` driver.

## 2. Spec basis

- `spec/spec.html#sec-try-statement-runtime-semantics-evaluation` (14.15.3),
  production `TryStatement : try Block Finally`: `Evaluation of Finally` is
  evaluated exactly **once** per evaluation of the `TryStatement`, regardless
  of why `B` (the Block's completion) is abrupt.
- `spec/spec.html#sec-runtime-semantics-catchclauseevaluation` (14.15.2),
  production `Catch : catch ( CatchParameter ) Block`: step 7, `Let B be
  Completion(Evaluation of Block)`; step 9, `Return ? B`. If the catch
  body's own evaluation is abrupt (here: a `Throw` completion with which
  `Await` resumes, per `spec/spec.html#await`, when its promise rejects —
  the resumed `Completion Record` becomes `Await`'s own return value at the
  point of resumption), that abrupt completion is returned unchanged — it is
  *not* caught a second time by the same `Catch`.
- `spec/spec.html#sec-try-statement-runtime-semantics-evaluation`, production
  `TryStatement : try Block Catch` (the inner try, which has no `Finally` of
  its own): `If B is a throw completion, let C be
  Completion(CatchClauseEvaluation of Catch with argument B.[[Value]])` ...
  `Return ? UpdateEmpty(C, undefined)`. When `CatchClauseEvaluation` itself
  returns an abrupt (`Throw`) completion, the inner `TryStatement` propagates
  that completion unchanged as its own completion — it has no `Finally` to
  run, so nothing happens at this level.

During planning, two further repros showed this is one bug *class* hitting
two independent code paths in the same driver, not one isolated spot (see
§4b for the second path and §5 for both fixes):
- A `return` statement after an `await` inside the same nested,
  finally-less catch body also double-runs the outer `finally` (confirmed by
  direct repro). The same `TryStatement : try Block Finally` production
  governs: `Finally` runs exactly once regardless of whether `B` (here, the
  inner try/catch's completion) is a throw or a return completion.
- A `throw` from the *inner* try's own `finally` block (not a nested catch)
  also double-runs the outer `finally` (confirmed by direct repro) — the
  same `TryStatement : try Block Finally` production applies with the inner
  try as the one whose own `Finally` already ran and produced the abrupt
  completion that becomes `B` for the outer try.

Chaining these: the inner `try/catch`'s abrupt completion becomes the
completion of the single statement that makes up the outer `try` Block; the
outer `try Block Finally` then runs `Finally` exactly once (per the first
clause above) and, since `Finally` completes normally, re-surfaces the
inner throw. The engine must behave as if the inner try/catch context were
already fully exhausted (no catch left to try, no finally of its own) by the
time the abrupt completion from the `await` resumption is routed — the outer
`finally` is the *first and only* handler left, and it must run once.

## 3. Files to touch

- `src/interpreter/eval.rs` — the `async_function_resume` driver, two spots:
  1. The "route pending exception through try stack" block (currently
     around lines 8665–8808, handler-selection/truncation around
     lines 8690–8796) — §4.
  2. The `route_return!` macro's `routed_to` branch (currently around
     line 8410) — §4b.

  This is the only production file that needs a change.
- `test262-extra/` — one new regression file (see §5); no existing
  `test262-extra` file covers any of the three nested shapes below.

No `docs/adr/` entry and no `CONTEXT.md` update: this is a localized bug fix
to existing exception-routing logic, not a new architectural decision or new
vocabulary.

## 4. Root cause (for the implementation stage)

`async_function_resume` keeps its own hand-written copy of "route a pending
exception to the nearest handler" (distinct from the shared
`route_generator_exception` helper in
`src/interpreter/eval/generator_runtime.rs:6366`, which sync generators and
async generators both go through). In the handler-selection block:

```rust
if let Some((depth, state, is_catch, _)) = handler {
    if is_catch {
        try_stack.truncate(depth + 1);
    } else if pending_completion_was_replaced {
        try_stack.truncate(depth + 1);
    }
    pending_exception = Some(exc);
    current_id = state;
    continue;
}
```

When the selected handler is a `finally` (`is_catch == false`) and no
pending `return`/loop-control completion was being replaced
(`pending_completion_was_replaced == false` — exactly the case for a
straightforward rejected `await`), `try_stack` is **not** truncated. In the
issue's repro, at the point the `await` rejects, `try_stack` holds two
entries: the outer `try/finally` (not yet entered) and the inner
`try/catch` (already `entered_catch = true`, no `finally_state`). The
handler search correctly walks past the exhausted inner entry and selects
the outer `finally` at depth 0 — but because nothing truncates the stack,
the inner entry stays on top. `StateTerminator::EnterFinally`'s handling
(`src/interpreter/eval.rs`, `if let Some(ctx) = try_stack.last_mut() {
ctx.entered_finally = true; }`) then marks the *wrong* (inner, already-spent)
entry as having entered its finally, leaving the outer entry's
`entered_finally` still `false`. The matching `StateTerminator::TryExit`
later pops that same wrong (inner) entry, so the outer entry survives with
`entered_finally` still `false` — which makes the exception-routing search
re-select the *same* outer `finally` handler a second time once the
restored `saved_finally_exception` resurfaces at `TryExit`, running the
`finally` body twice.

The shared `route_generator_exception` helper (used by sync and async
generators) does not have this bug: it truncates to `depth + 1`
unconditionally whenever *any* handler (catch or finally) is selected, with
no `pending_completion_was_replaced` special case. The async-function
driver's own `route_loop_control!` macro (`src/interpreter/eval.rs`, around
line 8430) gets this right too — `try_stack.truncate(depth + 1)` on the
routed branch, `try_stack.truncate(target.try_depth)` otherwise — which is
why the "break after an await in a nested finally-less catch" repro tried
during planning came out correct (single `"finally"`).

**Fix part 1 (the reported bug)**: make the exception-routing block do the
same thing `route_generator_exception` and `route_loop_control!` already
do — unconditionally `try_stack.truncate(depth + 1)` whenever a handler is
selected, whether it is a catch or a finally. Every entry above `depth` was,
by construction of the search loop just above, already exhausted (its catch
already entered or absent, its finally already entered or absent) — exactly
the contexts `route_generator_exception` already drops unconditionally — so
dropping them here is never lossy.

This leaves `pending_return_was_replaced`, `pending_loop_control_was_replaced`,
and `pending_completion_was_replaced` (current lines 8692–8695) dead for the
purpose of the truncate decision. The `.take()` calls on `pending_return`
and `pending_loop_control` must stay (a fresh throw still has to clear a
completion it is replacing), but the three `_was_replaced` bindings become
unused and must be removed, not left as dead code, to satisfy the repo's
`clippy -D warnings` pre-commit gate.

## 4b. A second, independent instance of the same bug class

Planning also directly reproduced (via `./target/release/jsse -e "..."`,
current branch tip, no production change made) the identical
double-`finally` symptom for a `return` statement placed where the issue's
`await` is — same outer `try/finally`, same inner finally-less `try/catch`,
`await null; return 'r';` in the catch body instead of a rejecting `await`:

```
async function f() {
  try {
    try { throw 1; } catch (e) { await null; return 'r'; }
  } finally { log.push('f'); }
}
```

logs `["f","f"]` before the function's promise resolves with `'r'`; it must
log `["f"]` once. This is a *different* code location with the *same*
missing-truncate shape: `route_return!` (`src/interpreter/eval.rs`, around
line 8362) searches `try_stack` in reverse for the nearest not-yet-entered
`finally_state` exactly like the exception-routing block does, but — unlike
`route_loop_control!` right below it in the same file, and unlike the fixed
exception-routing block — it never truncates `try_stack` to the selected
depth at all:

```rust
if let Some((_, finally_state)) = routed_to {
    pending_return = Some(ret_val);
    current_id = finally_state;
} else if ...
```

so a stale, already-exhausted entry (the same kind left behind by the
nested finally-less catch) stays on top of the stack, `EnterFinally` marks
the wrong entry again, and `TryExit` → `pending_return.take()` →
`route_return!` again re-selects the same still-unentered outer `finally`.

**Fix part 2**: add `try_stack.truncate(depth + 1);` to `route_return!`'s
`routed_to` branch, matching `route_loop_control!`'s own routed branch
immediately below it in the same file. This is folded into this PR rather
than deferred: it is the same one-line pattern, in the same function, found
in the course of fixing part 1, and leaving it unfixed would mean this PR's
own regression test suite (§5) documents a known-adjacent double-`finally`
bug without closing it.

A third shape tried during planning — `throw` from the *inner* try's own
`finally` body (no nested catch involved) — also reproduced the double-run,
but through the *same* exception-routing block that fix part 1 already
covers (the inner `finally`'s own, already-`entered_finally` context is the
stale entry left behind); no third fix location is needed, only another
test case (§5).

## 5. TDD slices

1. **Red**: add `test262-extra/async-function-nested-try-catch-rejected-await-runs-outer-finally-once.js`
   (pattern after the existing `test262-extra/async-function-loop-control-through-finally.js`:
   `esid: sec-try-statement-runtime-semantics-evaluation`, an `info:` block
   quoting the "Finally runs exactly once" + "CatchClauseEvaluation
   propagates an abrupt Block completion unchanged" reasoning from §2,
   `flags: [async]`, `features: [async-functions]`). Cover, in one file with
   sequential `.then()` chaining (matching the existing style), one
   `async function` per scenario:
   - the issue's exact shape (outer `try/finally`, inner `try/catch`, a
     rejecting `await` in the catch body) — asserts `finally` logged once
     and the function's promise rejects with the original error.
   - the same shape one level deeper (outer `try/finally`, a *middle*
     `try/catch` with no `finally`, an *innermost* `try/catch` with no
     `finally`, rejecting `await` in the innermost catch body) — asserts the
     outer `finally` still runs exactly once, directly exercising that
     `truncate(depth + 1)` drops more than one stale entry.
   - a sibling case where the inner catch body's `await` *resolves* rather
     than rejects — asserts normal completion still reaches the outer
     `finally` exactly once (guards the fix against breaking the
     non-exceptional path, which already routes through `TryExit` rather
     than this exception-handler-selection block).
   - a `throw` from the *inner* try's own `finally` body, no nested catch
     involved (§4b's third repro: outer `try/finally`, inner `try/finally`
     whose own finally throws) — asserts the outer `finally` runs exactly
     once. Exercises the same part-1 fix with a stale *finally* context
     (not a stale catch context) left on top.
   - a `return` after `await` inside the same nested, finally-less catch
     body (§4b's second repro) — asserts the outer `finally` runs exactly
     once and the function's promise resolves with the returned value.
     Exercises part 2 (`route_return!`).

   Run it against the unmodified tree with
   `uv run python scripts/run-test262.py test262-extra/async-function-nested-try-catch-rejected-await-runs-outer-finally-once.js`
   to confirm every scenario fails exactly as expected (doubled `"finally"`
   log entries) before touching production code.

2. **Green**: in `src/interpreter/eval.rs`:
   - Change the handler-selection block described in §4 so
     `try_stack.truncate(depth + 1)` runs unconditionally whenever `handler`
     is `Some`, regardless of `is_catch`; delete the now-dead
     `pending_return_was_replaced`, `pending_loop_control_was_replaced`, and
     `pending_completion_was_replaced` bindings (keep the bare
     `pending_return.take();` / `pending_loop_control.take();` side
     effects).
   - Add `try_stack.truncate(depth + 1);` to `route_return!`'s `routed_to`
     branch as described in §4b, mirroring `route_loop_control!`'s routed
     branch.

   Re-run the same `run-test262.py` invocation and confirm all scenarios in
   the new file pass.

3. **Refactor** (only if the green diff leaves an obvious seam — expected to
   be minimal since both changes are small): re-read the touched blocks for
   clippy cleanliness (unused `is_catch`/`depth` bindings if an `if/else`
   collapses, matching `route_generator_exception`'s shape) and run
   `./scripts/lint.sh`.

## 6. Test surface

- **Targeted test262 run** (regression-check that the fix does not move
  existing try/catch/finally or async-function behavior):
  - `uv run python scripts/run-test262.py test262/test/language/statements/try/`
  - `uv run python scripts/run-test262.py test262/test/language/statements/async-function/`
  - `uv run python scripts/run-test262.py test262/test/language/expressions/async-arrow-function/`
  - `uv run python scripts/run-test262.py test262/test/language/statements/async-generator/`
    and `test262/test/built-ins/AsyncGeneratorPrototype/` (not expected to
    change — these already pass the equivalent nested shape via
    `route_generator_exception` — run anyway since the touched driver shares
    the same `TryContextInfo` type and `StateTerminator` variants).
- **New `test262-extra` coverage** (§5, slice 1): test262 has no file
  exercising a rejecting `await` (or a `return`, or a nested `finally`'s own
  `throw`) nested two `try` levels deep inside a catchless `try/finally` —
  the closest existing coverage,
  `test262/test/language/statements/async-function/try-throw-finally-reject.js`,
  awaits in the *finally* itself with a single `try/finally` level, not a
  nested `try/catch` whose catch body awaits. Hence the new
  `test262-extra/async-function-nested-try-catch-rejected-await-runs-outer-finally-once.js`.
- **Existing `test262-extra` regression that must keep passing unchanged**:
  `test262-extra/async-function-loop-control-through-finally.js`'s
  `rejectedFinallyReplacesBreak` scenario is the one case that already
  exercised the `pending_completion_was_replaced` conditional before this
  fix (a `break`'s pending loop-control completion is replaced by a
  rejected `await` in a `finally`). It already truncates correctly today,
  and collapsing the conditional to an unconditional truncate for that same
  `depth` is a no-op there — but it is the one existing test that would
  catch a mistake in collapsing the branches, so re-run it explicitly:
  `uv run python scripts/run-test262.py test262-extra/async-function-loop-control-through-finally.js`.
- **Full gate before considering the fix done**: `cargo test --release` (unit
  tests, including anything under `src/interpreter/tests.rs` that exercises
  `async_function_resume`); `uv run python scripts/run-custom-tests.py`;
  `uv run python scripts/run-test262.py test262-extra/` (the default
  `run-test262.py` invocation with no path does **not** cover
  `test262-extra/` — it must be passed explicitly); then the full
  `uv run python scripts/run-test262.py` (no path = full default suite:
  `language/`, `built-ins/`, `annexB/`, `intl402/`) to confirm no regression
  against the `origin/main:test262-pass.txt` baseline.

## 7. Regression risk

- The touched block is inside `async_function_resume`'s main dispatch loop —
  a tree-walker/state-machine hot path exercised by every `async function`
  call that suspends at an `await`, so any syntax slip here is caught
  immediately by the full test262 run, not just the targeted directories.
- The change only affects the *exception-routing* branch (`pending_exception
  .is_some()` at loop entry) — it does not touch `StateTerminator::TryEnter`,
  `TryExit`, `EnterCatch`, or `EnterFinally` themselves, nor the parallel
  `for_of_stack`/`scope_stack` unwinding immediately above/below it in the
  same block. Those interact with `try_stack` depth bookkeeping
  (`scope_target`, `unwind_from` computed from `handler`'s `depth`) and must
  keep agreeing with the now-unconditionally-truncated stack; slice 1's
  deeper-nesting case is specifically there to catch a mismatch.
  - `scope_stack`/`for_of_stack` unwind targets are computed from `depth`
    directly (not from `try_stack.len()`), so they are unaffected by whether
    the stack above `depth` was already truncated — but worth re-checking
    after the change since they execute in the same block.
- `TryContextInfo`, `StateTerminator::TryEnter/TryExit/EnterCatch/EnterFinally`
  are shared AST/runtime types between the plain-async-function driver
  (`eval.rs`) and the generator/async-generator drivers
  (`eval/generator_runtime.rs`), but the fix only edits the async-function
  copy — it cannot regress sync- or async-generator behavior, which already
  goes through the unconditionally-truncating `route_generator_exception`.
- GC rooting / `gc_safepoint()`: `TryContextInfo` carries a
  `pending_completion: Option<PendingCompletion>` field, and
  `PendingCompletion::Return`/`Throw` hold a `JsValue` — so dropping a
  `TryContextInfo` can drop a GC-traced value. The fix does not change
  *when* a value-bearing entry is considered reachable: every entry this
  truncation newly drops is, by the search loop's own invariant, already
  exhausted (its catch/finally already entered or absent) exactly like the
  entries the `is_catch` branch already drops unconditionally today, so no
  entry holding a still-pending `pending_completion` that any live code path
  still reads is truncated away. Still worth a pass over
  `gc::trace_object_fields`/`gc_safepoint()` call sites that trace
  `try_stack` during the implementation stage to confirm nothing assumes
  the stack only ever shrinks by exactly one entry per `TryExit`.
- Bytecode fast path: not implicated — `async function` bodies with
  `await`/suspension go through the tree-walker's generator/async
  state-machine transform (`generator_transform.rs`), not
  `bytecode_enabled`'s compiled path.
- Node-compat library harnesses: no currently-green library
  (`decimal.js`, `big.js`, `acorn`, `prismjs`, `uglify-js`, `highlight.js`,
  `uuid`, `luxon`, `zod`, `moment`) is likely to exercise this exact nested
  try/catch-with-rejecting-await-in-catch shape, but re-run
  `./scripts/run-library-tests.sh` for any library touched by the same PR's
  CI gate as a cheap extra check; no library-specific change is planned.

## 8. Out of scope

- Unifying `async_function_resume`'s hand-written exception-routing copy
  with the shared `route_generator_exception` helper in
  `generator_runtime.rs`. The duplication predates this issue, is
  load-bearing for other async-function-specific behavior in the same
  function (dispose stacks, `for_of_protocol_failure`, `Completion::Exit`
  propagation for issue #242) that the generator helper does not handle,
  and de-duplicating it is a refactor, not a bug fix — not bundled here.
- Auditing every other `try_stack.truncate(...)` call site in
  `async_function_resume` for the same class of bug beyond what planning
  already checked. `route_loop_control!` (`src/interpreter/eval.rs`, around
  line 8430) was directly tested during planning with a `break` after an
  `await` in the same nested finally-less catch shape and already produces
  the correct single `"finally"` — it already truncates unconditionally on
  both its routed and unrouted branches, so it needs no change. The
  `unwind_for_of!` macro's own handler search (around line 8245) truncates
  via `try_stack.truncate(loop_state.try_depth)` on a different trigger
  (for-of iterator close) and was not exercised by any repro tried during
  planning; revisit only if a future report surfaces a similar double-run
  there.
- Any formatting-only or unrelated cleanup elsewhere in `eval.rs`.
