# Plan: issue #683 — async-function state machine has no block-scope states

## 1. Problem restated

`Interpreter::async_function_resume` (`src/interpreter/eval.rs:8178-9330`) drives a
lowered async-function body as a flat list of states with no runtime concept of a
block's own lexical scope: an ordinary `Statement::Block` is flattened straight
into the enclosing state graph at transform time
(`generator_transform.rs:872-874`), and the one construct that *does* get a real
per-block `Environment` — a block that directly declares `await using`
(`block_has_await_using`) — is special-cased by keeping the whole block AST
intact as the last statement of one state and tree-walking it verbatim
(`generator_transform.rs:847-875`), rather than being represented as scope-aware
state-machine data. That single mechanism has to serve two things it wasn't
designed for and breaks in three named ways: (1) if the intact block itself
contains an unrelated `await` (not part of the block's own disposal), resuming
re-enters the *same* state from the top (`eval.rs:8824` passes `current_id`, not
a fresh state) and replays the whole block — including re-declaring
`await using a = null` — forever; (2) `await using` declared directly in a
try/catch/finally clause's own statement list (no extra `{ }`) never reaches the
intact-block path at all, so its resource lands on the *function-level*
`func_env.dispose_stack` (state bodies execute directly against the passed-in
env with no child `Environment`, per `exec.rs:81-86`) and is disposed only at
function exit — after `finally` runs, not at the try block's own exit as the
spec requires; (3) an `await using` block nested inside another intact block
falls outside the single-slot suspend mechanism (`suspendable_dispose_block:
Option<usize>` / `parked_block_dispose: Option<DisposeCursor>`, `mod.rs:302-306`,
a pointer to exactly one AST node) and disposes via the blocking driver
(`run_dispose_cursor_blocking`, `dispose.rs:228-241`) instead of suspending. All
three are symptoms of the same missing primitive: the state machine has no
notion of "a block scope is currently open," so it can neither split a scope's
interior across further states nor track more than one open scope's disposal at
once, nor dispose a scope at its own natural exit rather than the function's.

## 2. Spec basis

- **`sec-block-runtime-semantics-evaluation`** (`spec/spec.html:21377-21410`,
  `Block : { StatementList }`): a `Block` creates its own Declarative
  Environment (`blockEnv`), evaluates its `StatementList` against it, and
  restores the outer `LexicalEnvironment` "no matter how control leaves the
  Block" — this is the scope boundary jsse currently only honors for the one
  AST shape it treats as an "isolated block."
- **`sec-try-statement-runtime-semantics-evaluation`** (`spec/spec.html:23182-23207`):
  for `try Block Finally`, step 1 evaluates `Block` to completion *first*
  (`_B_`), and only then evaluates `Finally` (step 2); `try Block Catch Finally`
  is the same shape with `CatchClauseEvaluation` interposed. Since a `Block`'s
  own evaluation (above) is responsible for disposing anything declared
  directly in it, a `try` body's `await using` resources must be gone before
  `Finally`/`Catch` ever runs — a `finally` clause is not the disposal
  boundary, the try `Block`'s own completion is.
- **`sec-runtime-semantics-catchclauseevaluation`** (`spec/spec.html:23150-23180`):
  `Catch : catch (CatchParameter) Block` likewise evaluates its own `Block`
  (with the same environment-creation/restoration shape) — a `catch` body's
  directly-declared `await using` is scoped to the catch clause, not the
  function.
- **Explicit Resource Management (`using`/`await using`, `DisposeResources`,
  `AddDisposableResource`) is not present in the pinned `spec/` submodule
  snapshot** (`spec/spec.html`, ecma262 commit `270a490b`, checked 2026-09-21:
  zero matches for `Dispose`/`UsingDeclaration` anywhere in the file) — it is
  authored and reviewed against the standalone
  `proposal-explicit-resource-management` spec text, exactly as the existing
  code already does: `dispose.rs:24` cites `sec-disposeresources` from that
  proposal, and `CONTEXT.md`'s "Isolated Block" entry documents the same
  `DisposeResources`/block-exit-disposal semantics this plan continues to rely
  on. This plan does not change that authority; it only fixes jsse's
  *implementation* of scope timing to match the `Block` boundary already cited
  above, which both the merged `Block` clause and the ERM proposal agree on
  (`test262-extra/await-using-lowering-preserves-block-scope.js` and
  `await-using-try-catch-finally-dispose-tick-alignment.js` already quote the
  relevant `DisposeResources`/`Block` steps in their `info:` fields).
- No JavaScript syntax changes. This plan changes only *when* an already-correct
  disposal runs and *whether* a suspension inside a block scope resumes
  correctly — never what `await using`/`try`/`Block` mean.

## 3. Files to touch

Scoped to **plain async functions** only (`Interpreter::async_function_resume`,
`eval.rs`). Sync generators (`generator_runtime.rs:482`,
`generator_next_state_machine_impl`) and async generators
(`generator_runtime.rs:3267`, `async_generator_next_state_machine_impl`) are
driven by two *separate* implementations that share none of `eval.rs`'s
`route_return!`/`route_loop_control!`/`unwind_for_of!` macros — extending them
is out of scope (§7) and left for a follow-up issue.

- `src/interpreter/types.rs` — new `ScopeFrame` struct (next to `ForOfLoopState`,
  `types.rs:397-424`) holding at least the block's own `Environment` and the
  `try_depth`/`for_of_depth` markers `route_return!`/`route_loop_control!` need
  to decide what's crossed; new `scope_stack: Vec<ScopeFrame>` field on
  `AsyncFunctionState` (`types.rs:376-394`).
- `src/interpreter/generator_transform.rs` — new `StateTerminator::EnterScope`/
  `ExitScope` variants (`StateTerminator` enum, `generator_transform.rs:76-143`);
  rewrite the `Statement::Block` arm of `transform_yielding_statement`
  (`generator_transform.rs:847-875`) so every block that directly declares
  `await using` is split through `EnterScope`/normal per-statement lowering of
  its interior/`ExitScope` — the intact-verbatim-block path is retired, not
  kept as a fast-path alternative (see §4 slice 1); extend whatever currently
  lowers try/catch/finally clause bodies so a clause body that directly
  declares `await using` (not via a further nested `Statement::Block`) also
  opens/closes a scope frame around its own statement list; update
  `clear_terminator_ic_sites` (`generator_transform.rs:173-233`, an explicit
  variant list, not a `_` wildcard) for the two new variants — neither carries
  an `Expression`, so both belong in its no-op arm (`227-231`, the explicit
  `Goto | LoopControl | TryExit | EnterFinally | Completed` list) alongside
  the variants that already need no IC-site clearing.
- `src/interpreter/generator_analysis.rs` — `has_block_with_await_using`
  (`845-857`, shallow, gates `transform_statements:783`) and
  `has_suspendable_await_using_block`/`scan_await_using`/`AwaitUsingScan`
  (`865-980`, deep, gates `stmt_has_suspension` at
  `generator_transform.rs:624` and the whole-function bailout at
  `generator_transform.rs:489`) both need to recognize a try/catch/finally
  clause body that directly declares `await using` as reachable/isolatable
  (today `scan_await_using`'s `Statement::Try` arm, `951-960`, only scans
  *through* the clause bodies for a further nested isolatable block via
  `scan_flattened_list`, never recognizing the clause body itself as one).
- `src/interpreter/eval.rs` — `async_function_resume` (`8178-9330`): new match
  arms for `EnterScope`/`ExitScope` in the terminator match
  (`8840-9328`, ends `StateTerminator::Yield{..} => unreachable!(...)` at
  `9326-9328`, no catch-all); extend `route_return!` (`8371-8413`),
  `route_loop_control!` (`8419-8461`), and the uncaught-throw routing block
  (`~8560-8681`) to unwind `scope_stack` frames crossed by the completion,
  modeled on how those same macros already unwind `for_of_stack` via
  `unwind_for_of!` (`8309-8368`); the isolated-block execution site
  (`8688-8717`) and the `suspendable_dispose_block`/`parked_block_dispose`
  single-slot mechanism it depends on (`mod.rs:302-306`) are replaced by
  pushing/popping real `scope_stack` frames. **The load-bearing line for bug
  2**: `term_env` (`8684-8687`, today `for_of_stack.last().map_or(&func_env,
  ForOfLoopState::effective_env)`) must also consult `scope_stack` — an
  `EnterScope` terminator has to create a real `NewDeclarativeEnvironment`
  child of whatever env was active and push it onto `scope_stack`, and every
  subsequent state body in that scope must execute against it, not `func_env`,
  or `await using` declared inside still lands on the wrong `dispose_stack`
  regardless of how the terminators are wired. Since `ScopeFrame` carries
  `for_of_depth` (§3), resolve `term_env` by comparing the innermost open
  scope frame's depth against the innermost open for-of loop's depth and
  taking whichever is deeper (a scope opened inside a loop body must win over
  the loop's iteration env, and vice versa) — define this rule at the
  `term_env` call site rather than leaving the interleaving for the
  implementation stage to discover.
- `src/interpreter/exec.rs` — the tree-walker's own `Statement::Block` handling
  (`~1060-1070`) currently special-cases the one pointer in
  `suspendable_dispose_block`; once nested blocks are reachable through real
  scope frames instead, this pointer-identity check is dead and should be
  removed, not left as an unreachable branch.
- `src/interpreter/dispose.rs` — `DisposeThen` (`164-175`, 4 variants) needs a
  5th variant (or an existing one generalized) carrying the state to resume at
  once a crossed scope's disposal finishes; the `(DisposeThen::Block, finished)`
  arm at `eval.rs:8527` is the precedent to extend, not the pattern to keep
  alongside an incompatible new one.
- `src/interpreter/gc.rs` — GC rooting for the new persisted `scope_stack`
  mirrors the existing `for_of_stack` touchpoints: `452-453`/`476` (walking a
  live stack), `744` (cleanup on instance removal), and the `for_of_stack`
  shared helper `collect_for_of_stack_roots` (`1135-1140`) is the template for
  an equivalent scope-stack helper.
- `docs/adr/` — new ADR, filename `docs/adr/YYYY-MM-DD-HHMM-async-block-scope-states.md`
  (UTC, per `docs/adr/README.md`'s current-naming convention), recording the
  `EnterScope`/`ExitScope`/`scope_stack` design, why it's scoped to async
  functions only, and the explicit non-goal of touching the two generator
  drivers.
- `CONTEXT.md` — the "Isolated Block" and "Block Exits" glossary entries
  (`CONTEXT.md`, Control flow section) describe the mechanism this plan
  replaces; update them to describe `scope_stack`/`EnterScope`/`ExitScope`
  once the old single-block mechanism is gone, so the glossary keeps matching
  the code.
- `test262-extra/` — new files per §4/§5 below, following the existing
  `await-using-*.js` header/harness pattern (`esid:`, `info:` quoting the exact
  spec steps, `flags: [async]`, `includes: [asyncHelpers.js, compareArray.js]`,
  `features: [explicit-resource-management]`, the `observe()`/witness-chain
  helper already defined in `await-using-lowering-preserves-block-scope.js` and
  `await-using-try-catch-finally-dispose-tick-alignment.js`).

## 4. TDD slices

1. **Red: the hang (bug 1).** Add
   `test262-extra/await-using-block-inner-await-suspends.js` containing
   *only* the issue's exact repro (`try { { await using a = null; await 0; }
   } catch(e){}`) — keep this file to the one hanging case, since a genuine
   hang is indistinguishable from "slow" and each case costs a full 120s
   timeout under `scripts/run-test262.py`'s per-test cap; that timeout *is*
   the red signal, do not build separate timeout infra. Add a sibling file,
   `await-using-block-inner-await-abrupt-variants.js`, for the throw-after-
   the-inner-`await` and second-`await`-after-`await using` cases, which
   should fail fast (wrong output, not a hang) once this slice's mechanism is
   in place. Green: add `ScopeFrame`/`scope_stack` (types.rs), `EnterScope`/
   `ExitScope` (generator_transform.rs, eval.rs), and change the
   `Statement::Block` transform arm (`generator_transform.rs:847-875`) so
   **every** block that directly declares `await using` is lowered through
   `EnterScope` (creating the block's own `Environment`, pushed onto
   `scope_stack`) + ordinary per-statement lowering of its interior +
   `ExitScope` (pop the frame, dispose its `dispose_stack` suspendably) —
   retire the intact-verbatim-block path entirely rather than keeping it as a
   fast-path alternative. Once a block's own declaration executes in one
   state and its `ExitScope` disposal may run in a different, later state,
   there is no shorter route to "the block's resource lives on the right env"
   than making that the *only* path; a kept-alive intact fast path duplicates
   the mechanism slice 3 needs and leaves the old pointer-identity branch in
   `exec.rs` for slice 5 to unwind instead of never introducing it.
2. **Red/green: abrupt completions crossing one open scope.** Extend the same
   test file (or a sibling) with `return` from inside the scope, an uncaught
   `throw` from inside it caught by an *outer* `catch`, and `break`/`continue`
   escaping it from inside a `while`. Green: extend `route_return!`,
   `route_loop_control!`, and the throw-routing block in
   `async_function_resume` to unwind `scope_stack` frames crossed by the
   completion before continuing to route it — model this on
   `unwind_async_for_of_loops`/`unwind_for_of!` (`eval.rs:8309-8368`,
   `~8624`), which already solves "possibly-suspending cleanup across
   multiple crossed frames" for `for_of_stack`; each crossed scope's
   disposal must itself suspend correctly (reuse `DisposeCursor`/
   `PendingDispose`, not `run_dispose_cursor_blocking`).
3. **Red: try-list disposal ordering (bug 2).** Add
   `test262-extra/await-using-try-list-dispose-before-finally.js` reproducing
   the issue's exact repro (`try { await using a = r; await 0 } finally {
   L('fin') }`, disposer must log before `'fin'`) plus the catch-list and
   finally-list equivalents. Confirm today it logs `'fin'` before the disposer
   runs (wrong order, not a hang). Green: extend
   `has_block_with_await_using`/`has_suspendable_await_using_block` /
   `scan_await_using`'s `Try` arm (`generator_analysis.rs:951-960`) to
   recognize a clause body that directly declares `await using`, and wrap that
   clause body's own statement list in `EnterScope`/`ExitScope` at transform
   time so its resource is disposed at the clause's own exit, before control
   reaches `Catch`/`Finally` — reusing the machinery slices 1-2 built rather
   than inventing a second scoping mechanism for this shape.
4. **Red: nested isolated blocks still block (bug 3).** Add a case to
   `test262-extra/await-using-block-dispose-tick-alignment.js` (or a new file)
   nesting an `await using` block inside another `await using`-bearing block
   or loop body, asserting tick-alignment via the same witness-chain technique
   used by the existing tests in that file. Confirm today the nested block's
   disposal drains the microtask queue inline (wrong tick count) rather than
   suspending. Green: retire the single-slot
   `suspendable_dispose_block`/`parked_block_dispose` mechanism
   (`mod.rs:302-306`, the tree-walker check at `exec.rs:~1060-1070`) now that
   `scope_stack` can hold more than one open scope, so a nested scope also
   gets the suspending `DisposeCursor` path.
5. **Refactor.** With slices 1-4 green, delete now-dead code: the
   `suspendable_dispose_block`-era pointer-identity check in `exec.rs`, the
   `GeneratorState.block_exits`/`BlockExits` table
   (`generator_transform.rs:27,38-42`, populated `864-868`, consumed
   `eval.rs:8769-8810`) if `scope_stack`-based routing fully subsumes it, and
   any `DisposeThen::Block` call sites left unreachable. Update the
   `CONTEXT.md` glossary entries (§3). Write the ADR. Run the full quality gate
   (§5/§6) once more before committing.

## 5. Test surface

- **New tests, `test262-extra/`** (this behavior is not covered by upstream
  test262 — confirmed by inspection: `test262/test/language/statements/
  await-using/` and `.../using/` contain only a `syntax/` subdirectory each,
  no runtime-semantics tests for disposal timing; `built-ins/DisposableStack`
  and `built-ins/AsyncDisposableStack` cover the built-in methods, not this
  lowering interaction. `test262/test/staging/explicit-resource-management/`
  also exists but is out of the default runner's scope per this repo's own
  `CLAUDE.md` — it is excluded deliberately, not missed, and running it
  explicitly is not part of this plan):
  - `await-using-block-inner-await-suspends.js` (slice 1)
  - additions covering abrupt completions (slice 2, folded into the same file
    or a sibling — implementer's call once the cases are drafted)
  - `await-using-try-list-dispose-before-finally.js` (slice 3)
  - an addition to `await-using-block-dispose-tick-alignment.js` for nested
    isolated blocks (slice 4)
- **Targeted test262 runs** (regression guard, not new coverage):
  `test262/test/language/statements/await-using/`,
  `test262/test/language/statements/using/`,
  `test262/test/language/statements/try/`,
  `test262/test/language/statements/async-function/`,
  `test262/test/built-ins/AsyncDisposableStack/`,
  `test262/test/built-ins/DisposableStack/`.
- **Full regression, every slice**: `uv run python scripts/run-test262.py`
  against the baseline read from `origin/main:test262-pass.txt` (never rewrite
  it — see the repo's own constraint), `uv run python
  scripts/run-custom-tests.py`, `cargo test --release` (covers `dispose.rs`'s
  own unit tests, `tests/*.rs`, and `src/interpreter/tests.rs`), and
  `./scripts/lint.sh`.
- **Existing test262-extra files that must stay green** (they exercise
  adjacent paths this change touches directly):
  `await-using-lowering-preserves-block-scope.js` (the `AwaitUsingScan::Blocked`
  cases — must keep working exactly as today unless a slice deliberately
  narrows the gate, in which case this file gains cases, never loses
  coverage), `await-using-block-abrupt-exit-routing.js`,
  `await-using-try-catch-finally-dispose-tick-alignment.js` (its 5 cases are
  the nested-`{}` sibling of slice 3's new direct-in-list cases — both shapes
  must keep agreeing on tick counts), `await-using-switch-case-block-dispose-
  tick-alignment.js`, `await-using-loop-body-dispose-tick-alignment.js`,
  `await-using-fn-level-suspends-at-dispose.js`,
  `await-using-dispose-suspended-gc-rooting.js` (GC rooting across a suspended
  disposal — directly relevant once `scope_stack` frames must also be rooted,
  §3/§6), `await-using-module-nested-block-dispose-tick-alignment.js`,
  `module-using-abrupt-completion-disposal.js`.

## 6. Regression risk

- **`test262-pass.txt` baseline**: every slice touches
  `Interpreter::async_function_resume`, the single driver for *every* async
  function in jsse, not just ones using `await using` — a mistake in
  `route_return!`/`route_loop_control!`'s new scope-unwinding step, or in the
  new `EnterScope`/`ExitScope` terminator handling, risks regressing ordinary
  async/await control flow broadly (async functions with plain `try`/`catch`/
  loops and no disposal at all). Run the full test262 suite after every slice,
  not only at the end.
- **GC rooting**: `scope_stack` frames will hold live `Environment`/`JsValue`
  data across a real suspension (the function returns to the event loop while
  a frame is open). Missing a rooting touchpoint (§3, `gc.rs:452-453/476/744/
  1135-1140`) is a use-after-collection bug that a normal test run may not
  reliably surface — `await-using-dispose-suspended-gc-rooting.js` already
  exists for exactly this concern and should gain a scope_stack-crossing case.
- **The two other state-machine drivers**: `StateTerminator` is shared by
  `generator_transform.rs`'s single lowering pass across sync generators,
  async generators, and async functions (`transform_async_function`
  rewrites `await`→`yield` before the shared pass and `Yield`→`Await` after,
  `generator_transform.rs:~2607-2615`). Adding `EnterScope`/`ExitScope` to the
  enum forces new match arms in `generator_runtime.rs`'s two driver functions
  (`877-1838` sync, `4307-5815` async) purely for exhaustiveness, even though
  slices 1-4 only ever *emit* the new variants when lowering a plain async
  function. Follow the codebase's own existing precedent for this shape —
  `eval.rs:9326-9328` already has `StateTerminator::Yield{..} =>
  unreachable!(...)` in the async-function driver for a variant only
  generators emit — and make the two new generator-side arms `unreachable!()`
  stubs, with a slice-1/3 test asserting emission stays gated to plain async
  functions (a generator or async generator containing the same block shapes
  must keep taking its existing, unrelated path unchanged). Guard the
  emission site itself, not just the tests: add a `debug_assert!` where
  `EnterScope` is emitted confirming the body being lowered is a plain async
  function (not a generator or async generator) — a panic in a debug build
  during library-harness testing is far cheaper to diagnose than the same gap
  surfacing as a silent wrong-behavior report from `run-library-tests.sh`.
- **`DisposeThen`'s new/generalized variant**: `(disposal.then, done)` is
  matched exhaustively at `eval.rs:8526-8539`; get the new variant's resume
  target wrong and a scope's disposal either resumes at the wrong state
  (silently wrong control flow, not a crash) or panics on an unmatched
  combination — cover this directly in slice 2's abrupt-completion tests,
  don't rely on the happy-path tests to catch it.
- **Bytecode fast path**: `bytecode/` is feature-flagged off by default
  (`bytecode_enabled`) and async-function state-machine lowering is a
  tree-walker concern; confirm the flag stays off for these tests so no
  bytecode-path interaction needs to be designed here.
- **Node-compat library harnesses**: async functions using `try`/`await` are
  common in the pinned libraries (`moment`, `zod`, `luxon`, etc.); a `route_
  return!`/`route_loop_control!` regression could show up there before it
  shows up in test262. Re-run at least `./scripts/run-library-tests.sh zod`
  and `./scripts/run-library-tests.sh moment` (both have exact Node
  cross-checks already wired) after the full slice sequence, not as a
  substitute for test262.

## 7. Out of scope

- **Sync-generator and async-generator equivalents of this bug.** `#665`
  already tracks the general "blocking driver instead of suspend" class for
  loop/switch heads in those drivers; the `EnterScope`/`ExitScope` mechanism
  this plan builds for `eval.rs`'s async-function driver does not extend to
  `generator_runtime.rs`'s two separate driver implementations, which have no
  `route_return!`/`route_loop_control!`/`unwind_for_of!` macros to hook into
  (they use a different per-terminator idiom, `route_generator_exception` and
  friends). A follow-up issue should decide whether to port the same
  `ScopeFrame` idea there or design something native to that driver's shape.
- **Broadening `has_suspendable_await_using_block`'s gate beyond what slices
  1-4 need.** The issue notes the fix "would also let" the
  `AwaitUsingScan::Blocked` cases (`for (let ..)`, `for-in`, `with`, a lexical
  declaration beside an isolatable block in the same list) be lowered instead
  of falling back to the tree-walker. That is a real follow-on improvement
  once `scope_stack` exists, but it touches more containers than the three
  named bugs require and should be its own PR with its own
  `has_suspendable_await_using_block` unit-test additions
  (`generator_analysis.rs:1116-1183`), not bundled into this fix.
- **Top-level module `await using` and async-generator `await using`.**
  Mentioned only as adjacent territory in `#665`, not in this issue's repros;
  `module-using-abrupt-completion-disposal.js` already covers today's
  (blocking) behavior and is not expected to change here.
- **Formatting/refactor cleanup unrelated to the scope-stack change** — e.g.
  any unrelated simplification noticed in `eval.rs`'s ~1150-line
  `async_function_resume` while working in it. Note it for a separate cleanup
  PR rather than folding it into this bug fix.
