# Plan: issue #665 — `await using` blocks nested in try/loop/switch still drain microtasks inline at disposal

## 1. Problem restated

After #645/#666, function-level and *state-isolated* block-level `await using` in async functions suspend at
DisposeResources' `Await`s through a `DisposeCursor`. A block is state-isolated only when the generator transform
emits it as the last statement of its own state (`transform_yielding_statement`, `Statement::Block` arm). The
transform only reaches such a block when it is a direct child of a statement list, or nested in `if`/labeled/plain
blocks: `has_block_with_await_using` (`generator_analysis.rs:844`) has arms for `Block`/`If`/`Labeled` only, and
`contains_suspension` looks at a `Variable`'s *initialiser*, never at the `await using` kind. So for
`try { { await using a = null; throw 1; } } catch (e) {}` all three predicates in `transform_statements`
(`generator_transform.rs:570-578`) are false, the whole `try` is emitted verbatim, runs in the tree-walker, and its
block reaches `exec_statement(Block)` -> `dispose_resources` -> `run_dispose_cursor_blocking`, which drains the
microtask queue inline. Queued jobs then run in the middle of synchronous code and the function's promise settles
on the wrong tick. The doc comment on `has_block_with_await_using` ("For/try/switch handle disposal internally")
is the false premise being corrected.

The same blocking driver is also used by async generators, `for`/`for-of` heads and iterator close, and the other
`await_value` callers. Those are **not** the same root cause (see §9) and are split into follow-ups; this PR is the
minimal first slice.

### Baseline evidence (release build of HEAD `d3c785f1`, witness chain `w1..w4` started before the call, vs Node 26.5)

| shape (all `await using a = null` in a nested block) | jsse today | Node / spec |
|---|---|---|
| try body, throws | `w1,caught-1,sync-end,w2,settled,...` | `sync-end,w1,caught-1,w2,settled,w3,w4` |
| try body, `finally` | `body,w1,fin,after,sync-end,...` | `body,sync-end,w1,fin,after,w2,settled,w3,w4` |
| catch body | `c,w1,after,sync-end,...` | `c,sync-end,w1,after,w2,settled,w3,w4` |
| finally body | `t,f,w1,after,sync-end,...` | `t,f,sync-end,w1,after,w2,settled,w3,w4` |
| while body (2 iters) | `b1,w1,b2,w2,after,sync-end,...` | `b1,sync-end,w1,b2,w2,after,w3,settled,w4` |
| for(var) body (2 iters) | `b0,w1,b1,w2,after,sync-end,...` | `b0,sync-end,w1,b1,w2,after,w3,settled,w4` |
| for-of(var) body (2 iters) | `b1,w1,b2,w2,after,sync-end,...` | `b1,sync-end,w1,b2,w2,after,w3,settled,w4` |
| switch case containing a block | `b,w1,after,sync-end,...` | `b,sync-end,w1,after,w2,settled,w3,w4` |
| block containing `break` in a loop | `b0,w1,w2,after,sync-end,...` | `b0,sync-end,w1,w2,after,w3,settled,w4` |

Also diverging today, **deferred** (§9): `for (await using a = ...;;)`, `for (await using a of ...)`, async generator
block and body-level `await using`.

### Pre-existing defects found while probing (they constrain the design — §6)

1. **Hang**: `async function f(){ while (true) { await 0; { await using a = null; if (++i > 2) break; } } }`
   never terminates (also labeled `break outer`/`continue outer`). The isolated block is emitted verbatim; its
   raw `Completion::Break/Continue` reaches the executor (`eval.rs:8759-8779`), whose fallback only knows the
   for-of stack, so the jump is dropped and the loop re-runs. `return` and `throw` out of the block work.
2. **Hang**: an `await using` block that *also contains an `await`* (`{ await using a = null; await 0; }`) never
   terminates: `rewrite_stmts_await_to_yield` turns the inner `await` into a `yield` inside the verbatim block, the
   executor suspends and re-runs the same state forever. Not caused by, nor fixed by, this issue -> follow-up A.
3. **The state-machine transform flattens block scope.** Lowered code loses per-iteration/lexical scoping:
   `while(i<3){ let j=i; fs.push(()=>j); await 0; i++ }` -> `2,2,2`; `for(let i..){ fs.push(()=>i); await 0 }` ->
   `3,3,3`; `let x=1; { let x=2; await 0; log(x) } log(x)` -> `2,2`; catch param shadowing likewise;
   `var i; for (let i..) { await 0 }` throws a redeclaration; `transform_for_in_statement` emits
   `Statement::Empty`, so a `for-in` containing an `await` is *silently skipped* (`for (k in {a,b}) { await 0; log(k) }`
   prints nothing). Today none of this touches a loop/try whose only "suspension" is an `await using` block,
   because that statement is not lowered and the tree-walker scopes it correctly.

Defect 3 is the critical design input: **widening lowering to every container of an `await using` block would
regress currently-correct code** (closure capture, shadowing, for-in). The plan therefore widens *only* to containers
for which flattening is unobservable (§6, "scope-safety gate"), and leaves the rest on the current blocking path.

## 2. Spec basis

DisposeResources / `await using` are from proposal-explicit-resource-management and are **not in the pinned `spec/`
commit** (`grep -n "DisposeResources\|await using" spec/spec.html` is empty). The existing tests/comments cite the
proposal ids; this plan does the same and cites `spec/spec.html` for the host clauses the proposal splices into:

- proposal `sec-disposeresources` — steps 3.d (Await(undefined) barrier before a sync disposer following a null
  resource), 3.f.ii (`needsAwait`), 4 (single trailing `Await(undefined)` when `needsAwait` and not `hasAwaited`);
  already implemented as `DisposeCursor` (`src/interpreter/dispose.rs`), unchanged by this plan.
- proposal Block Evaluation (amends `sec-block-runtime-semantics-evaluation`, spec.html:21377): `blockValue` is
  passed through `DisposeResources` before the block's completion is returned — so a `break`/`continue`/`return`/
  `throw` leaving the block disposes first, then continues as that completion.
- `await` (spec.html:51047): each `Await` costs exactly the promise-reaction ticks that the witness chains pin.
- Host constructs whose control flow the transform must preserve: try (`sec-try-statement-runtime-semantics-evaluation`,
  23182), for (`sec-runtime-semantics-forloopevaluation`, 22021), for-in/of body evaluation
  (`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`, 22388), switch
  (`sec-switch-statement-runtime-semantics-evaluation`, 22939), `sec-asyncblockstart` (51009),
  `sec-runtime-semantics-evaluateasyncfunctionbody` (25409), `sec-execute-async-module` (27254).
- Switch case clauses: jsse's parser (`src/parser/statements.rs:8,17`, `in_switch_case`) and Node both reject
  `using`/`await using` directly in a `case` list, so the switch-scope dispose sites `exec.rs:2503-2546` can never
  hold an async resource and never `Await`. They need no change; the issue's "switch heads" wording is over-broad
  (only a `Block` *inside* a case can hold `await using`). Do **not** change parser behaviour here.

Authority order for expected tick arrays: spec-derived (each DisposeResources `Await` = 1 tick, as in
`test262-extra/await-using-block-dispose-tick-alignment.js`), cross-checked against Node 26.5 (`node` is the reference
oracle only; both agree on every array in §1).

## 3. Scope of this PR (and what "closes" means)

In: async functions and top-level-await modules (they share `async_function_resume`, `mod.rs:3843`), for
`await using` **blocks** reached through `try`/`catch`/`finally` bodies, `while`/`do-while`/`for` (non-lexical init)/
`for-of`/`for-await-of` bodies, `switch` case lists, labeled statements, `if`, and plain nested blocks — subject to
the scope-safety gate — plus correct `break`/`continue` routing out of an isolated block.

Out (each a follow-up, §9): async generators, loop heads and iterator-close disposal, `for (let ..)`/`for-in`/`with`
containers, `await using` declared *directly* in a try/catch/finally list, nested `await using` inside an isolated
block, blocks with an inner `await` (hang), other blocking `await_value` sites.

Because the issue as written spans all of these, the PR must say **"Refs #665"** (not `Closes`) and #665 stays open
with a progress comment; closing it would falsely claim async generators/heads are done. Suggested PR title
(squash subject): `fix(disposable): suspend nested await using blocks at disposal in try/loop/switch bodies`.

## 4. Files to touch

- `src/interpreter/generator_analysis.rs` — new `has_suspendable_await_using_block` (tri-state scan, §6);
  fix the stale doc comment on `has_block_with_await_using`; unit tests for the scan.
- `src/interpreter/generator_transform.rs` — `GeneratorState.block_exits` + `BlockExits` type; record exits when
  isolating a block; make `stmt_has_suspension` (l.514) and the entry gate (l.389) consult the new scan for async
  *functions* only (`detect_for_await == true`; async generators keep the shallow predicate — their executor cannot
  route the new raw completions); unit tests.
- `src/interpreter/eval.rs` (`async_function_resume`, ~8759-8779) — resolve `Completion::Break/Continue` from an
  isolated block through `block_exits` and `route_loop_control!`, before the existing for-of fallbacks.
- `test262-extra/` — new files (§5); no existing file changes.
- `src/interpreter/tests.rs` — optional engine-level regression for the hang if a `#[test]` harness fits.
- `CONTEXT.md` — add **Isolated Block** (an `await using` block emitted intact as the last statement of its state so
  the executor can park its `DisposeCursor`) and **Block Exits** (transform-time break/continue target table for it).
- No `docs/adr/` entry: no architectural decision beyond extending #666's mechanism (the scoped-block-states redesign
  in follow-up A *will* need an ADR).

Do not touch: `dispose.rs` (cursor semantics), `exec.rs` dispose sites, async-generator/sync-generator executors,
`spec/`, `test262/`, `test262-pass.txt`.

## 5. Test surface

New files under `test262-extra/` (test262 frontmatter, `flags: [async]`, `includes: [asyncHelpers.js, compareArray.js]`,
`features: [explicit-resource-management]`, `esid: sec-disposeresources`, reuse the `observe` witness-chain helper
from `await-using-block-dispose-tick-alignment.js`). Run with
`uv run python scripts/run-test262.py test262-extra/` (there is no dedicated runner).

- `await-using-try-catch-finally-dispose-tick-alignment.js` — try body (throw / no throw), catch body, finally body;
  expected arrays in the §1 table.
- `await-using-loop-body-dispose-tick-alignment.js` — while, do-while, for(var), for-of(var), for-await-of over an
  array, labeled loop; 2 iterations each.
- `await-using-switch-case-block-dispose-tick-alignment.js` — block inside a case, with and without `break`.
- `await-using-block-abrupt-exit-routing.js` — `break`, `continue`, labeled `break outer`/`continue outer`, `return`,
  `throw` out of an isolated block inside a loop that also `await`s; assert result values **and** that the disposer ran
  before the jump (disposer logs) and in reverse order for two resources. Includes the two hangs from §1 (defect 1).
- `await-using-lowering-preserves-block-scope.js` — the characterization set (all **green on baseline**, verified with
  a release build): C1 `for (let i..){fs.push(()=>i); {await using a=null;}}` -> `0,1,2`; C2 `while`+`let j`+closure
  -> `0,1,2`; C3 `for (k in {a,b})` with an isolated block -> `a,b`; C4 `let x=1; try { let x=2; {await using ..} log(x) }
  finally{} log(x)` -> `2,1`; C5 `var i='outer'; for (let i..) {{await using ..}} log(i)` -> `outer`.
- Module: one `flags: [module, async]` case via a `_FIXTURE.mjs` imported dynamically (pattern:
  `module-using-abrupt-completion-disposal.js`) with `await using` block inside a top-level `try`/`while`; assert
  relative order (body -> dispose -> after) and that a witness job interleaves. Only pin exact ticks if they are stable
  across a fresh Node run.

Rust unit tests: scan table in `generator_analysis.rs`; state-shape tests in `generator_transform.rs` (a `try`
containing an await-using block lowers to `TryEnter` + an isolated state with `block_exits`; a `for (let..)` container
does not).

Targeted test262 (must not regress; run before the full suite):
`test262/test/language/statements/{await-using,using,try,for,for-of,for-await-of,while,do-while,switch,labeled,block,async-function,async-generator}/`,
`test262/test/language/expressions/{await,async-function,async-generator}/`,
`test262/test/language/module-code/top-level-await/`, `test262/test/built-ins/{AsyncDisposableStack,DisposableStack}/`.
The generator/async-function areas are included because the widened predicate changes which function bodies get the
full state machine. Then the full run: `uv run python scripts/run-test262.py`, plus `uv run python scripts/run-custom-tests.py`,
`cargo test --release`, `./scripts/lint.sh`. (Submodules are empty in a fresh workspace:
`git submodule update --init --depth 1 test262 spec`.)

## 6. TDD slices (red -> green -> refactor)

Each slice is one reviewable commit; run only the named test until green, then the quick gates.

**Slice A — tripwires first (green before any production change).** Add
`await-using-lowering-preserves-block-scope.js` (C1-C5) and the *existing-behaviour* halves of the loop/try files that
already pass (values, not ticks). Confirm green on the untouched build. Purpose: they turn red the moment lowering is
widened unsafely, which is the main regression vector of this issue.

**Slice B — break/continue out of an isolated block (fixes defect 1).**
RED: `await-using-block-abrupt-exit-routing.js` (hangs today — validate RED with
`timeout 10 target/release/jsse <script>`, not through the 120 s harness).
GREEN: in `generator_transform.rs` add
`struct BlockExits { breaks: HashMap<Option<String>, LoopControlTarget>, continues: HashMap<Option<String>, LoopControlTarget> }`
(behind `Rc`) and `GeneratorState.block_exits: Option<Rc<BlockExits>>`; in the `Statement::Block` arm of
`transform_yielding_statement`, when isolating and `ctx.detect_for_await`, snapshot `ctx.break_targets`/`ctx.continue_targets`
into the state being finalised. In `async_function_resume`'s `Completion::Break(label,_)`/`Completion::Continue(label,_)`
arms consult `state_machine.states[current_id].block_exits` first and call `route_loop_control!(target)`; keep the
existing for-of fallbacks for verbatim breaks (#672). Chosen over a new `StateTerminator` variant because the async-
and sync-generator executors match `StateTerminator` and must stay untouched. `route_loop_control!` already runs
intervening `finally` blocks and closes crossed for-of iterators, so no new unwinding logic is needed. Note the
disposal has already happened (cursor -> `DisposeThen::Block` -> `preloaded_stmt_result`), matching the proposal order.

**Slice C — the scan (pure function, no behaviour change yet).**

Rules, with explicit precedence (the implementer must not re-derive them):

1. A `Block` that itself directly declares `await using` is **Isolatable**. It is *not* descended into and does *not*
   count as a lexical declaration of the list that holds it: it stays intact as the last statement of its own state
   and is never flattened. A loop/`if`/`try` body that *is* such a block is the isolatable unit, not a flattened list.
2. Every list the transform would *flatten* (try block, catch body, finally list, a plain non-isolated block, a
   `switch` case list) combines its elements' results. If the combined result is Isolatable and that same list directly
   holds any *other* lexical declaration (`let`/`const`/`using`/`await using`/class/function), the whole container
   becomes **Blocked** (flattening would be observable). A list with no nested Isolatable stays `None` (never lowered,
   so its declarations are irrelevant).
3. `for (let|const|using|await using ..;;)` init, any `for-in`, and `with` are Blocked whenever their body is not
   `None`. `for-of` heads are not gated (ForOfHead creates a per-iteration env). Catch *parameters* are not gated (the
   headline repro needs them; the shadowing flaw is shared by every lowered try today — defect 3, follow-up B).
4. Mixed containers (one Isolatable path, one Blocked path) are Blocked as a whole. The scan never descends into
   function/class bodies. `has_suspendable_await_using_block(stmt) == (scan(stmt) == Isolatable)`.

Derivation the tests must encode (this is the proof the gate is neither too tight nor too loose):
- all nine §1 rows -> Isolatable: the loop bodies `{ await using a = null; L(..) }` are rule-1 blocks; try/catch/finally
  and switch-case rows hold a bare `{ await using .. }` Block as the only element, so rule 2 finds no sibling declaration.
- C1 `for (let i..)` and C5 -> Blocked (rule 3, init); C2 `while` body `{ let j=i; fs.push(..); { await using .. } i++ }`
  -> Blocked (rule 2, `let j` beside the nested block in a flattened body list); C3 `for-in` -> Blocked (rule 3);
  C4 `try { let x=2; { await using .. } .. }` -> Blocked (rule 2, `let x` in the try list).

RED: unit tests in `generator_analysis.rs` asserting the above plus: `None` for a container with no await-using block
and for `for await` without one; Isolatable for labeled/`if` branches/plain nested blocks/`for(;;)`/`for(var ..)`/
`for-of` with `var|let|const` heads/`for await` with a nested block. Follow the module's existing hand-built-AST
convention (`Statement::Variable { .. }`); if that is too noisy for the nested cases, `Parser::new(src)` is reachable
(`src/interpreter/tests.rs:8` uses it) — pick one and keep it consistent.
GREEN: implement the tri-state scan (`None | Isolatable | Blocked`) per the rules above.

**Slice D — wire the scan into the transform (headline repro goes green).**
RED: `await-using-try-catch-finally-dispose-tick-alignment.js` (arrays in §1).
GREEN: in `generator_transform.rs` make `stmt_has_suspension` return true when
`is_async && detect_for_await && has_suspendable_await_using_block(stmt)` (this single change propagates to every
per-construct body check — if 1665/1679/1708/1722, while 1783, do-while 1836, for 1955, for-of 2061, switch 2240,
labeled 2283 — so none of them needs its own edit; verify by reading each site once). Use the same disjunct at
`transform_statements` (l.576) and the fast-path gate (l.389, so an async function whose only trigger is such a block
gets the full machine instead of `create_simple_machine`). Async generators (`detect_for_await == false`) keep
`has_block_with_await_using`. `transform_try_statement` already lowers its three lists via `transform_statements`,
so `try` needs no code beyond the predicate.
Refactor: update the doc comment on `has_block_with_await_using` to say what it does and does not cover.

**Slice E — loops and switch.**
RED: `await-using-loop-body-dispose-tick-alignment.js`, `await-using-switch-case-block-dispose-tick-alignment.js`.
GREEN: should pass from slice D (`for await` is already lowered today and its body block may already be isolated, so
that row is "verify, may already pass" — it still needs the routing from slice B); fix whatever the lowered loop/switch forms expose *only if it is a defect of the
newly lowered path*. If a case exposes a defect-3-class flaw, tighten the gate (make it `Blocked`) rather than fixing
the transform in this PR.

**Slice F — module and full verification.** Add the module fixture test; run every gate in §5; compare the full
test262 run against the `origin/main` baseline **without** `--update-baseline`; confirm `test262-extra/` is 100%.

**Slice G — housekeeping (in the same PR, last commit).** `CONTEXT.md` terms; post the #665 progress comment
(§9); `git rm PLAN.md` before opening the PR.

## 7. Regression risk

- **Widening changes which bodies get the full state machine.** `has_block_with_await_using` gates the
  `create_simple_machine` bail (l.389). Only async functions/modules that contain a suspendable await-using block
  change path, but that is a behaviour and perf change for exactly those bodies; the targeted run in §5 includes the
  async-function/await/generator/module trees for that reason.
- **Scope flattening (defect 3).** The scope-safety gate exists solely to avoid spreading it. Residual exposure:
  catch-param shadowing in a newly lowered `try`, and `var`/`let` name collisions the gate cannot see. Slice A
  tripwires cover the known shapes.
- **Interaction with #672** (switch case break/continue lowering, l.2240) — same predicate; the switch tests must keep
  the `break`-in-case behaviour.
- **Shared machinery leaned on:** `async_function_resume`, `route_loop_control!`/`unwind_for_of!`, `DisposeCursor`
  parking (`DisposeThen::Block`, `suspendable_dispose_block`), GC rooting of parked cursors
  (`with_gc_root_scope`/`gc_root_frame`; `BlockExits` holds no `JsValue`s), the `StateTerminator` exhaustive matches
  (untouched — that is why exits are a side table, not a variant). No `ObjectKind`/`property.rs`/bytecode changes; the
  bytecode fast path is off by default and async state-machine bodies run in `async_function_resume`, not the VM.
- **Baseline movement:** expected none in `test262-pass.txt` (tick alignment is not test262-covered — that is why the
  new files live in `test262-extra/`). Any newly failing test262 test is a blocker, not something to re-baseline.
- **Node-compat library harnesses:** run `./scripts/run-library-tests.sh zod` (heavy async use) as a smoke check;
  no dependence on `await using`.

## 8. Ordering / dependencies

A -> B -> C -> D -> E -> F -> G. B (routing) must land before D: widening turns loops that today run correctly
in the tree-walker into lowered loops, and without routing a `break` inside their isolated block hangs.

## 9. Out of scope — follow-ups to file (each its own issue, label `needs-triage`; list them in the #665 comment)

- **A. Scoped block states for the async-function state machine** (root cause of defects 2-3): `EnterScope`/`ExitScope`
  states with a saved scope stack (as `for_of_stack` does for iteration envs), unwound by `route_return!`/
  `route_loop_control!`/exception routing. Would fix: `await using` block containing an `await` (hang),
  nested `await using` blocks inside an isolated block (still blocking), `await using` directly in a try/catch/finally
  list (disposed at function exit, after `finally`: `try { await using a = r; await 0 } finally { L('fin') }` logs
  `fin` before the disposer — Node logs the disposer first), and lets the gate be removed. Needs an ADR.
- **B. Lowered-loop/try scoping defects** independent of `await using`: per-iteration `let` bindings (`3,3,3`, `2,2,2`),
  block/catch-param shadowing (`2,2`), `for-in` containing an `await` silently skipped (`transform_for_in_statement`
  emits `Statement::Empty`), `var i; for (let i..){await 0}` redeclaration error.
- **C. Loop heads and iterator close:** `for (await using a = ..;;)` and `for (await using a of ..)` disposal
  (exec.rs:1866/1933/2316, `close_for_of_loop`, `unwind_async_for_of_loops`, the head `iteration_env.take()` +
  `dispose_resources` in `async_function_resume`, `eval.rs:9126`); the head `await using` resource is currently emitted
  into `term_env` and may attach to the wrong environment. Also: `for (await using a of [null]) { L('b') }` logs `b`
  two ticks late (`sync-end,w1,w2,b,...` vs Node `b,sync-end,...`) — verify whether the head performs a spurious
  Await.
- **D. Async generators:** block-level parking needs a cursor slot in `IteratorState::StateMachineAsyncGenerator`
  (GC-traced) and resumption via `async_gen_await_resume`; body-level disposal (9 `dispose_resources` sites in the async-generator half of
  `generator_runtime.rs`, spec `sec-asyncgeneratorstart`/`sec-asyncgeneratordrainqueue`) needs the return/throw/complete
  paths to suspend before settling the request. Bring the break/continue routing over at the same time.
- **E. Other blocking `await_value` callers** (`eval.rs:933,1019`, `exec.rs:2232`, `generator_runtime.rs` yield*/for-await
  sites): same "drain inline" class but a different mechanism from DisposeResources; `Expression::Await` evaluated
  by the tree-walker outside a lowered state is one instance. Separate audit.
- **F. Module `for (await using x of ..)` not detected by `module_has_tla`** (`mod.rs:4241`, `for-of` arm ignores the
  head kind) — such a module takes the synchronous path. Verify and fix separately.
- `class static block` (`literals.rs:1212`), `eval` (`eval.rs:6144`), plain function-call (`eval.rs:5891`) and the sync
  module (`mod.rs:3767`) `dispose_resources` callers: synchronous contexts that cannot `Await`; no change.
- Formatting/rename/refactor of `async_function_resume` or `dispose.rs`; changing parser rules for `case` clauses;
  moving `test262-pass.txt`.

## 10. Autonomous-run mechanics

- Before implementing, verify slice-A tripwires are green and record the hang repros in a `gh issue comment 665`
  (planning stage already posted the findings and split; the implementation stage appends the follow-up issue links).
- If the scan/gate proves too conservative to cover the headline repro and loops in slice E, do **not** relax it to
  make a test pass; ship the covered subset and record the gap in the PR body and follow-up A.
- Commit hygiene: conventional-commit subjects, `git rm PLAN.md` before opening the PR, `Refs #665`.
