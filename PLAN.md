# Plan: issue #665 — second slice: `for (await using x of …)` heads in async functions and TLA modules

## 0. Where #665 stands (read first)

PR #688 (squash `da44e1e6` on `main`) already landed the **first** slice of #665: nested `await using`
*blocks* in try/catch/finally, loop bodies and switch-case blocks of async functions/TLA modules
suspend at their disposal `Await`, and `break`/`continue` route out of an isolated block. The issue was
left open on purpose; the residual scope was split into #683 (block scope states), #684 (lowered
loop/try scoping), #685 (loop heads + iterator close), #686 (async generators), #687 (other blocking
`await_value` callers).

This branch's HEAD (`e6fcad54`) is the merged PR #688 head, and `origin/main` is 4+ commits ahead
(#690/#691/#692 touched `generator_transform.rs`; #691 made lowered `for-in` run). Earlier automated
comments claiming "no further PR can be opened from this branch" were wrong: a **new** PR is fine.

**Implementation stage, step 0:** `git fetch origin main && git rebase --onto origin/main e6fcad54 HEAD`
(replays only this plan commit; then `git rm PLAN.md`). Do not plain-`rebase origin/main`: the squash
merge makes the old commits conflict. Build with `cargo build --release -j4` (no `cargo` parallelism
beyond that; ~1m40 cold on this host). Re-run every repro below on the rebased binary before trusting
any number in this file; all numbers were taken on `origin/main` = `30b1535d`.

**Slice choice.** The park-the-cursor machinery (`PendingDispose` / `DisposeThen` /
`Scheduler::park_async_function_dispose`, `eval.rs` `async_function_resume`) exists only for
`AsyncFunctionState`. Async generators have no equivalent field — #686 needs new state on the async
generator object and the `async_gen_await_resume` path (11 `dispose_resources` sites in
`async_generator_next_state_machine_impl`), so it is the expensive slice. `for-of` heads are lowered to
`StateTerminator::ForOfHead` inside `async_function_resume`, which already parks cursors, so this slice is
cheap **and** it exposes a spec bug that has to be fixed first (below). This PR: `Refs #665`, `Refs #685`
(not `Closes` — iterator-close exits and `for(;;)` heads remain).

## 1. Problem restated

`for (await using x of iterable) body` in an async function (or a top-level-await module) does not
behave like a plain `for-of` whose iteration environment is disposed with `await`:

1. **Parse bug (observable value change).** `parse_for_statement` hard-codes `is_await: true` in the
   `for (await using x of …)` branch (`src/parser/statements.rs:~789`), ignoring the real `for await`
   flag parsed at `:590`. So the head is run as a `for await`: values are awaited via
   AsyncFromSyncIterator (a thenable element is unwrapped: `x !== thenable`) and the first body runs two
   ticks late. And `for await (await using x of …)` is also parsed through this branch, so the real
   flag is dropped for it. Probe on `main`:
   `for (await using a of [th]) seen = a === th` → jsse `false`, node `true`.
2. **Blocking disposal.** Per-iteration `DisposeResources` at the `ForOfHead` state
   (`eval.rs` `StateTerminator::ForOfHead`, `self.dispose_resources(&disp_env, …)`) drains the microtask
   queue inline, so queued jobs run mid-synchronous-code and the async function settles on the wrong tick.
   Node vs jsse on `main` (witness chain `w1..w8` started before the call, `L` logs):

   | shape | node | jsse (main) |
   |---|---|---|
   | `for (await using a of [null]) {L('body')} L('after')` | `body,sync-end,w1,after,w2,settled` | `sync-end,w1,w2,body,w3,w4,w5,after,w6,settled` |
   | resource with async disposer | `body,disp,sync-end,w1,after,w2,settled` | `sync-end,w1,w2,body,disp,w3,w4,w5,after,…` |
   | two iterations, disposers | `body,disp,sync-end,w1,body,disp2,w2,after,w3,settled` | `sync-end,w1,w2,body,disp,w3,w4,w5,body,disp2,w6,w7,w8,after,settled` |
   | body containing `await 0` | `sync-end,w1,body,w2,after,w3,settled` | `sync-end,w1,w2,w3,body,w4,w5,w6,after,w7,settled` |

   Experiment (scratch tree, not committed): changing only the parser to pass the real flag fixes item 1
   (`plain: a === th → true`, `for await: false`, and `for await (await using…)` matches node's trace
   exactly) and turns item 2 into the plain blocking pattern `body,w1,after,sync-end,…` — i.e. the loop is
   then *not lowered* and the tree-walker (`exec.rs` for-of, `dispose_resources` at `:~2301`) drains inline.
   So the analysis must also treat the head as a suspension point (slice 2 below).

Out of this PR (still diverge; tracked): break/return/throw exits (`close_for_of_loop`,
`unwind_for_of!`), `for (await using …;;)` heads (`exec.rs` `:~1866/1933`), async generators, and the
other blocking `await_value` callers — see §7.

## 2. Spec basis

`spec/` (ecma262 `270a490b`) predates Explicit Resource Management: it has no `await using`
`ForDeclaration` and no DisposeResources. The normative text for the new pieces is the
[proposal-explicit-resource-management] spec; its algorithms are reproduced in the `info:` blocks of the
test262 files cited below and by the existing `test262-extra/await-using-*` files
(`esid: sec-disposeresources`). Base clauses that do exist in `spec/`:

- **`sec-for-in-and-for-of-statements`** — grammar: `for ( ForDeclaration of AssignmentExpression )` and
  `for await ( ForDeclaration of … )` are **separate productions**; the proposal adds
  `ForDeclaration : await using ForBinding` to both. The un-awaited production has `iteratorKind` *sync*.
- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`** —
  step 1 (`iteratorKind` not present ⇒ `sync`); `nextResult = Call(next)`; "If `iteratorKind` is `async`,
  `Await(nextResult)`" (so `for (await using …)` must NOT await `nextResult` or the value); proposal step 4
  (`IsAwaitUsingDeclaration` ⇒ hint `async-dispose`) and steps 9.j–k
  (`Set result to Completion(DisposeResources(iterationEnv.[[DisposeCapability]], result))` after each
  iteration's body).
- **`sec-disposeresources`** (proposal) — step 3.f (`needsAwait`), step 4 (single trailing `Await(undefined)`
  when needed and `hasAwaited` is false), and `Dispose` step 3 (`Await(result)` for `async-dispose`). Implemented by `DisposeCursor` (`src/interpreter/dispose.rs`), unchanged.
- **`await`** (`sec-await` anchor is `#await` in `spec/spec.html`) — the disposal `Await` suspends the running
  async function context; the continuation is a later job. That is what parking implements.

test262 that pins the semantics (already in tree, must stay green):
`test262/test/language/statements/for-of/head-await-using-*.js`,
`test262/test/language/statements/for-await-of/head-await-using-init.js`,
`test262/test/language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-each-iteration-of-forofstatement.js`.

## 3. Files to touch

- `src/parser/statements.rs` — `for (await using x of …)` branch (`:~772-790`): set `is_await` from the
  local `for await` flag instead of `true`. (No other parser change; check the `for (await using x = …;;)`
  branch is untouched.)
- `src/ast.rs` — add a tiny helper on `ForOfStatement`, e.g. `fn awaits_at_head(&self) -> bool`
  (`is_await` **or** left is `Variable(decl)` with `decl.kind == VarKind::AwaitUsing`), with a doc comment
  saying `is_await` alone means the `for await` iteration protocol.
- `src/interpreter/generator_transform.rs` — `stmt_contains_for_await` (`:~550`) and `stmt_has_suspension`
  (`:~619`): use `awaits_at_head()` so the loop is lowered; leave `transform_for_of_statement` (`:~2229`) and
  the `:~2707` site passing the strict `f.is_await` to `ForOfHead`.
- `src/interpreter/generator_analysis.rs` — if `contains_suspension`/`has_suspendable_await_using_block`
  interact (the `ForOf`/`Using|AwaitUsing` arm at `:~938-950` returns `blocked_unless_none` for the body):
  keep behavior, add unit tests; do not widen block isolation here.
- `src/interpreter/mod.rs` — `stmt_has_tla` `Statement::ForOf` arm (`:~4273`): `fo.awaits_at_head()`
  (today it relies on the buggy `is_await`, so a module with only `for (await using …)` must keep being
  detected as TLA after the parser fix).
- `src/interpreter/dispose.rs` — add `DisposeThen::ForOfIteration` (or reuse a neutral name): "iteration
  environment of a for-of head finished disposing; resume the head state".
- `src/interpreter/eval.rs` (`async_function_resume`):
  - `StateTerminator::ForOfHead` arm: replace the blocking `self.dispose_resources(&disp_env, Completion::Empty)`
    with `take_dispose_stack(&disp_env)` → set
    `pending_dispose = Some(PendingDispose { cursor: DisposeCursor::new(stack, Completion::Empty), then: DisposeThen::ForOfIteration })`
    and `continue` (the top-of-loop `pending_dispose` block steps it; suspension is at `current_id` = the head).
  - top-of-loop `DisposeStep::Done` match: `(DisposeThen::ForOfIteration, Completion::Throw(e)) => pending_exception = Some(e)`
    (same routing the head used inline); `(ForOfIteration, _) =>` fall through so the head re-enters and
    finds `iteration_env == None`. Keep the `Completion::Exit` arm ahead of it (issue #242).
- `docs/` / `CONTEXT.md` — add one glossary line if a new term is introduced (`Head Disposal`:
  "DisposeResources of a `for-of` iteration environment, parked at the `ForOfHead` state"); no ADR needed.
- Tests: §4/§5.

Not touched: `exec.rs`, `generator_runtime.rs` (async-generator / sync-generator executors),
`close_for_of_loop`, `unwind_for_of!`, `scheduler.rs` (parking API already generic),
`test262-pass.txt`, `spec/`, `test262/`.

## 4. TDD slices (red → green, one commit each; conventional-commit subjects)

Every RED test is written and run on the rebased binary first; each slice ends with
`cargo test`, `./scripts/lint.sh`, and the targeted runs in §5. Use `uv run python scripts/run-test262.py <path>`
for `test262-extra/`. Do not `&&`-chain gate commands.

1. **`fix(parser): for (await using x of …) is a sync for-of`** —
   RED: `test262-extra/await-using-for-of-head-sync-iteration.js`
   (`esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`,
   `features: [explicit-resource-management, async-iteration]`, flags `[async]`, `asyncHelpers.js`).
   Asserts: (a) with `th = { then(r){ r(null) }, [Symbol.asyncDispose]() {} }`,
   `for (await using a of [th]) seen = a === th` ⇒ `true` (the element is not awaited); (b) the contrast
   `for await (await using a of [th])` ⇒ `false` (AsyncFromSyncIterator unwraps the thenable).
   GREEN: `statements.rs` one-liner.
   Add a parser unit test asserting `is_await == false` / `true` for the two heads (`src/parser` tests or
   `src/interpreter/tests.rs`, wherever AST-shape tests already live).
   Expected interim state after this commit alone: correct values, blocking ticks — fine, the next slice
   is the tick fix (do not push in between).
2. **`fix(async): lower for (await using … of …) heads`** —
   RED: Rust unit tests in `generator_analysis.rs`/`generator_transform.rs` test modules: an async body whose
   only "suspension" is `for (await using x of y) {}` is **not** `create_simple_machine`-eligible and produces
   a `ForOfHead` terminator; `Interpreter::module_has_tla` (or `stmt_has_tla`) is true for a module whose only
   TLA is that head. GREEN: `awaits_at_head()` in `ast.rs` + the three call sites. (`for await` behavior
   must be byte-identical: `awaits_at_head` ⊇ `is_await`.)
3. **`fix(disposable): suspend at each for-of iteration disposal in async functions`** —
   RED: `test262-extra/await-using-for-of-head-dispose-tick-alignment.js`
   (`esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`, info quotes
   ForIn/OfBodyEvaluation 9.j–k + DisposeResources 3.f/4, flags `[async]`, `compareArray.js`), same witness
   harness as `await-using-try-catch-finally-dispose-tick-alignment.js` (`observe(shape)`), asserting the
   **node column above** for: null resource; object with async disposer; two iterations; body with `await 0`;
   a throwing disposer caught by an outer `try` (`caught-…` before `after`, iteration count stops); a
   rejecting disposer promise. GREEN: `DisposeThen::ForOfIteration` + the two `eval.rs` edits.
   Add a GC-rooting regression next to `await-using-dispose-suspended-gc-rooting.js` (a for-of iteration whose
   disposer allocates and calls `gc()`/allocation churn while parked) — the parked cursor is already traced
   through `gc.rs:~401`; the test proves the new park site keeps the iterator, the iteration env and the
   cursor alive.
4. **`test(disposable): pin top-level for (await using … of …) head disposal ticks in modules`** —
   `test262-extra/await-using-module-for-of-head-dispose-tick-alignment.js` + `_FIXTURE.mjs`, copying the
   layout of `await-using-module-nested-block-dispose-tick-alignment.js` (`flags: [module, async]`,
   `dynamic-import`). Expected trace = node's on the equivalent module (compute with `node --input-type=module`).
   If RED, the module TLA path needs the same `ForOfHead` change — verify it runs through
   `async_function_resume` (mod.rs `:~3720-3767`); fix in this slice only if it is a call-site gap, else
   split.
5. **Refactor (optional, only if trivial):** if the two `pending_dispose` construction sites now share three
   lines, extract `fn park_cursor(stack, completion, then)`; otherwise leave. No other refactors.

Run `test262/test/language/statements/for-await-of/` after slice 2: `for await` lowering must be unchanged.

## 5. Test surface

Targeted test262 (run each, compare against `origin/main:test262-pass.txt`, no regressions):

- `test262/test/language/statements/for-of/`
- `test262/test/language/statements/for-await-of/`
- `test262/test/language/statements/await-using/`
- `test262/test/language/statements/using/`
- `test262/test/language/statements/for/` (heads sharing `parse_for_statement`)
- `test262/test/language/statements/async-function/`, `.../async-generator/`, `test262/test/language/expressions/await/`
- `test262/test/language/module-code/top-level-await/`
- `test262/test/built-ins/DisposableStack/`, `.../AsyncDisposableStack/`
- Then the full default run: `uv run python scripts/run-test262.py` (never rebuild the binary during it).

Not covered by test262 ⇒ new `test262-extra/` files above (spec clause named in `esid`/`info`):
sync-vs-`for await` element awaiting for the `await using` head; tick alignment of head disposal; module
variant; GC rooting of a parked for-of disposal. Run the whole dir:
`uv run python scripts/run-test262.py test262-extra/` (must stay 100% green), plus `cargo test`
(lib + bin; the fmt/clippy hook blocks on dead code — land the new `DisposeThen` variant together with its
two `eval.rs` uses) and `uv run python scripts/run-custom-tests.py`.

Probe script for the implementer (compare jsse vs node before/after; witness chain of 8 promise reactions,
`L` logs, `sync-end` logged after the call): the shapes in §1's table plus `for await (await using a of [null])`
(unchanged, must equal node) and `agen_*` shapes (must be unchanged — out of scope, see §7).

## 6. Regression risk

- **Parser change is global to `for (await using x of …)` and `for await (await using x of …)`.** Both
  previously got `is_await: true`. Effects: values are no longer awaited for the plain form (correct);
  `for await (await using …)` now gets its real flag. Consumers of `ForOfStatement.is_await`:
  `generator_transform.rs` (`:550, :619, :2229, :2707`), `exec.rs` (`:2181, :2216` tree-walker for-of),
  `mod.rs:4273` (TLA), and via the transform `eval.rs`/`generator_runtime.rs` `ForOfHead`. Any site that
  used `is_await` to mean "this loop has an await" must move to `awaits_at_head()`; grep once more for
  stragglers (including `hoisting.rs`, `gc.rs`, `types.rs` matches on `ForOf`).
- **Async generators / sync generators** now see the plain `for (await using x of …)` as *sync*; their
  per-iteration disposal stays blocking (`generator_runtime.rs:~5518`) but the element-awaiting bug goes
  away. `agen_forof_head` trace changes from `w1,w2,body,…` to `body,w1,…` — it is still ≠ node's
  pre-#686 ticks; note that in the PR, do not pin it.
- **Hot paths:** `async_function_resume` (`eval.rs`), `generator_transform.rs` lowering predicates. The
  `awaits_at_head` predicate is evaluated at transform time only (once per function), not per execution.
  `exec_statement`/`eval_expr` untouched. Bytecode compiler bails on `statement:ForOf`
  (`bytecode/compiler.rs:771`), so no bytecode-path change.
- **GC:** the parked cursor is traced via `AsyncFunctionState.pending_dispose` (`gc.rs:~401`); the iteration
  environment lives in `ForOfLoopState.iteration_env` (already a root in the saved `for_of_stack`) — but the
  head does `iteration_env.take()` before parking, so the env's dispose stack must be rooted by the cursor
  only: covered by the GC-rooting regression test.
- **Baseline movement:** expect `for-of/head-await-using-*`, `for-await-of/head-await-using-init`,
  `await-using/initializer-*-each-iteration-of-forofstatement` to stay green; any new failure in
  `for-await-of/` or `module-code/top-level-await/` is a slice-2 regression (a TLA module that is no longer
  detected as async). Do not touch `test262-pass.txt`; the runner diffs against `origin/main`.
- **Node-compat library harnesses:** none of `decimal.js`/`big.js`/`acorn`/… use `await using`; no run needed
  beyond `cargo test`. (`zod`/`moment`/`luxon` also unaffected.)

## 7. Out of scope (do not bundle)

- **Iterator-close exits** — `break`/`return`/`throw` leaving a `for (await using x of …)`:
  `close_for_of_loop` (`eval.rs:~9394`) and the `unwind_for_of!` macro disposing inline. Needs a resumable
  unwind: a `DisposeThen::ForOfClose { loop_state, unwind_from, resume }` continuation carrying what the macro's
  caller does next (break target / continue target / return value / rethrow). Follow-up on #685 (slice B).
- **`for (await using x = …; …; …)` heads and iterations** — `exec.rs:~1866/1933/2301` (tree-walker,
  unlowered): depends on the loop-scoping work in #684.
- **Async generators** (#686): needs `pending_dispose` on the async-generator state, parking in
  `async_gen_await_resume`, and queue interplay (`async_gen_process_queue`); 11 dispose sites.
- **Other blocking `await_value` callers** (#687): `eval.rs:933,1019`, `exec.rs:2217`,
  `generator_runtime.rs:3093,3316,3499,3713,4247,5589,6118,6156`.
- **Block scope states** (#683), **lowered loop/try scoping** (#684).
- Any formatting/cleanup outside the touched lines; no `test262-pass.txt` update; no ADR.

## 8. PR / issue hygiene

- Title: `fix(disposable): suspend at for-of head disposal and parse for (await using … of …) as sync iteration`
  (squash subject). Body: `Refs #665`, `Refs #685`; list the node-vs-jsse trace table; state what remains.
- Leave a `gh issue comment 665` recording the slice choice (async generators need new state; for-of heads
  reuse the parked-cursor machinery; and the `is_await: true` parse bug found) and update #685's description
  with the remaining close-exit / `for(;;)` items.
