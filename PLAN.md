# Plan: issue #686 — `await using` in async generators drains microtasks inline at disposal

Base: `origin/main` @ `a129a725` (#703). The branch was fast-forwarded to it during planning
(it was 6 commits behind; #694/#699/#703 rewrote the files this touches). `spec/` and `test262/`
were initialised (`git submodule update --init --depth 1 spec test262`) and a release build exists
in `target/release/jsse` (`cargo build --release -j4`, ~1.5 min) — re-run it after the first edit.

## 1. Problem restated

Every DisposeResources site in the async-generator driver
(`async_generator_next_state_machine_impl`, `src/interpreter/eval/generator_runtime.rs`) calls
`dispose_resources` -> `run_dispose_cursor_blocking` (`dispose.rs`), which `await_value`s by draining
the microtask queue inline. The async-function driver already suspends at each disposal `Await`
(`PendingDispose`/`DisposeCursor`, `scope_stack`, ADR-2026-09-21-1007); the async-generator driver
has no equivalent. Measured on this HEAD with `target/release/jsse` vs `node` (probe scripts in the
session; each is a self-contained `.js`):

| shape | jsse | node |
|---|---|---|
| fn-level `await using`, disposer awaits a `setTimeout` promise, then completes (`j.js`) | `d-start\|n2:true\|d-end` — request settles **before** the disposer finishes (`await_value` returns `undefined` once the queue empties) | `d-start\|d-end\|n2:true` |
| `return 5` after a yield, fn-level resource (`ret.js`) | disposer starts synchronously inside `.next()` (`before-return\|d-start\|w1\|sync-end`) | `before-return\|sync-end\|w1\|d-start` — `Await(5)` comes first |
| block `{ await using a; yield 1; }` (`blk.js`) | `body\|d\|x1\|body\|after-yield\|d\|post` — disposed at the *yield*, block **replayed**, disposer runs twice | `body\|x1\|after-yield\|d\|post` |
| `try { await using a; yield 1 } finally {}` (`n.js`) | `body\|x1\|after-yield\|f` — **disposer never runs** (regression from #703: `557f67da` prints `…\|f\|d`, late) | `…\|after-yield\|d\|f` |

The last two are correctness bugs, not tick misordering: the intact-block path
(`Statement::Block` async-generator arm, `generator_transform.rs`) emits the block verbatim as one
tree-walked statement even when it contains `yield`/`await`, and after #703 an `await using`
declared in a lowered `try`/`finally` list lands in a `generator_scope_stacks` frame that nothing
ever disposes. **Bisected**: `557f67da` disposes late (after `finally`, ADR bug 2), `a129a725` (#703)
drops the disposal entirely — a regression `main` currently ships.

### The issue text is partly stale — the plan deliberately deviates

- "cursor slot in `IteratorState::StateMachineAsyncGenerator`": a new variant field means editing 28
  literal constructions in `generator_runtime.rs` + `eval.rs:5529` + `types.rs`
  (`completed_state_machine_async_generator`, whose unit test asserts every field is cleared) and
  splitting the shared sync/async or-pattern at `gc.rs:1109`. #703 set the cheaper precedent for
  driver state that lives outside the `IteratorState` enum: `generator_for_of_stacks` and
  `generator_scope_stacks` are `FxHashMap<u64, _>` side tables on `Interpreter`, GC-rooted in
  `collect_gc_roots` and removed in `free_gc_object`. **Decision: the parked cursor is a side table
  `generator_pending_dispose: FxHashMap<u64, …>` beside them.** Alternative (variant field) rejected
  for churn; it remains a mechanical swap if a reviewer insists.
- "break/continue routing from #665's `BlockExits`": `BlockExits` has **no producer** since #701
  (only `block_exits: None` initialisers and dead readers remain) and `suspendable_dispose_block`/
  `parked_block_dispose` were **never** wired in the async-generator driver, so the CONTEXT.md
  "Isolated Block"/"Block Exits" entries describing them as async-generator machinery are wrong.
  There is nothing to "come over".
- "Block-level parking of the intact block": parking a cursor on the intact block would not fix
  `blk.js`/`n.js` (a block containing `yield` is replayed regardless). **Decision: stop emitting the
  intact block for async generators and port the async-function scope-state design
  (`EnterScope`/`ExitScope`) instead**; both generator drivers already call
  `reconcile_scope_stack` and already have the `generator_scope_stacks` table, so only the two
  terminator arms (currently `unreachable!`, `generator_runtime.rs:1856` and `:5934`), the transform
  gate, and disposal-on-crossing are missing.

## 2. Spec basis

`spec/` (tc39/ecma262) has **no** explicit-resource-management text (`grep -c "await using"
spec/spec.html` = 0); DisposeResources / `await using` are governed by the ERM proposal, which is
what `dispose.rs` already cites (`proposal-explicit-resource-management`, `sec-disposeresources`,
`sec-runtime-semantics-blockdeclarationinstantiation`, the `Block`/`FunctionBody`/`ForBodyEvaluation`
DisposeResources hooks). The N/A hatch does not apply. Ordering constraints are grounded in
clauses that *are* in `spec/spec.html`:

- `sec-asyncgeneratorstart` — body result -> `AsyncGeneratorCompleteStep(result, true)` (settles the
  request **directly**, no extra Await) -> `AsyncGeneratorDrainQueue`. DisposeResources of the
  function body happens before this, as part of evaluating the body.
- `sec-return-statement-runtime-semantics-evaluation` — `return expr;` in an async generator does
  `? Await(exprValue)` **before** the return completion propagates, i.e. before any disposal
  (jsse currently disposes first).
- `sec-asyncgeneratoryield` / `sec-asyncgeneratorunwrapyieldresumption` — `.return(v)` resumption
  at a yield awaits `v` before unwinding (jsse does not; see out-of-scope).
- `sec-asyncgeneratorawaitreturn`, `sec-asyncgeneratordrainqueue`, `sec-asyncgeneratorcompletestep`
  — queue advance after a request settles; a parked disposal keeps its request at the queue front
  exactly as a parked body `Await` does, so a later `next()` cannot start the generator early.
- `sec-block-runtime-semantics-evaluation` — a block owns a fresh environment per entry; its
  disposal is at block exit (ERM step) on every completion kind.
- test262 anchors (already in `test262-pass.txt`, must stay green):
  `language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-asyncgeneratorbody.js`,
  `…Symbol.dispose-called-at-end-of-asyncgeneratorbody.js`, `…-called-at-end-of-block.js`,
  `…-each-iteration-of-forofstatement.js`, `…-forstatement.js`.

Node is used only as a tick-order cross-check for the new tests; every expected sequence must also
be derivable by counting Awaits from the clauses above.

## 3. Files to touch

Engine:
- `src/interpreter/dispose.rs` — new `DisposeThen` variants the async-generator tails need
  (`Complete`, `Return`, `Throw`, `ScopeExit(usize)`, `ForOfIteration`, `ScopeCross*` already
  exist and are reusable; add only what is missing, e.g. a return-operand-await step), plus a
  small `PendingDispose`-holding entry type for the side table.
- `src/interpreter/mod.rs` — `generator_pending_dispose` field + init (next to
  `generator_scope_stacks`, mod.rs:265).
- `src/interpreter/gc.rs` — root the parked cursor (`cursor.for_each_value`), the parked request's
  `promise`/`resolve_fn`/`reject_fn`, and any scope-frame envs (existing
  `collect_scope_stack_roots`); remove the entry in `free_gc_object` (next to :749).
- `src/interpreter/eval/generator_runtime.rs` — async half only:
  - new helpers: park a cursor at its `Await` (register `then` closures that call a new
    `async_gen_dispose_resume`), resume/step it, and the common "request settled -> `pop_front` ->
    `async_gen_process_queue`" tail (mirror `async_gen_await_resume`, ~:5961);
  - rewrite the 11 `dispose_resources` sites (line numbers on main: 4075 top-of-loop throw, 4175
    top-of-loop `pending_return`, 4265 statement throw, 4840 `return expr` throw, 4941 `return expr`,
    5120 bare `return;`, 5169 Throw terminator, 5235 ConditionalGoto, 5384 SwitchDispatch, 5538
    ForOfHead iteration env, 5809 Completed) to park instead of block;
  - implement the `EnterScope`/`ExitScope` arms (replace the `unreachable!` at ~:5934) and
    disposal of crossed scope frames on throw/`return`/`.return()`/`.throw()`;
  - fix `Completion::Exit` from disposal at the sites that `unreachable!`/swallow it (top-of-loop
    ~:4075, Completed ~:5809).
- `src/interpreter/generator_transform.rs` — new gate (see group B, sub-commit 2d) so `is_async` generators route
  await-using blocks and try/catch/finally clause lists through `transform_scope_block` /
  `transform_clause_body`; static `ExitScope` chain for `break`/`continue` leaving await-using
  scopes (`jump_terminator` at ~:468 only emits `LoopControl` for plain async functions; generators
  get `Goto`, which would let `reconcile_scope_stack` drop the frame *without disposing it*).
  **Do not overload `detect_for_await`** — it currently means "plain async function" (used at
  :469, :550-553, :559 for `LoopControl`, the for-await/simple-machine short-circuit); add a
  separate flag (working name `emit_scope_states`) and audit each of the ~14 `detect_for_await`
  reads to decide which meaning they need.
- `src/interpreter/generator_analysis.rs` — reuse `has_suspendable_await_using_block` /
  `scan_await_using` for async generators (currently reachable only with `detect_for_await`).
- `src/interpreter/exec.rs` — only if `reconcile_scope_stack`/`Block` arm need a hook; the
  tree-walker `Block` arm's `suspendable_dispose_block` check stays as is for async functions.

Tests: see §5. Docs:
- `CONTEXT.md` — rewrite **Scope Frame** to cover async generators; delete **Isolated Block** and
  **Block Exits** (both stale, see §1) or reduce them to what remains true for the
  `AwaitUsingScan::Blocked` fallback.
- `docs/adr/2026-09-2x-….md` — new short ADR superseding the "Scoped to plain async functions"
  section of ADR-2026-09-21-1007 (do not edit the accepted ADR beyond a one-line "superseded in
  part by" pointer). Records: side-table for the parked cursor, scope states for async generators,
  static `ExitScope` chain for break/continue, `return expr` Await-before-dispose.

## 4. TDD slices (red -> green, commit after each; branch stays shippable after each)

Every slice starts by writing the test in `test262-extra/` (§5) and watching it fail on the current
binary, using the witness-chain pattern from
`test262-extra/await-using-fn-level-suspends-at-dispose.js` (promise-reaction chain started before
the call, position of "settled" pins the tick count). Run the test with
`uv run python scripts/run-test262.py test262-extra/<file>` (memory:
`run-test262-extra-tests.md`). Cross-check the expected sequence against `node`.

**Framing.** `n.js` is a live regression on `main`: bisected — at `557f67da` it prints
`body|x1|after-yield|f|d` (disposed late, after `finally`, ADR bug 2); at `a129a725` (#703) it prints
`body|x1|after-yield|f` (never disposed). #703 made `try`/`finally` clause lists open an
`OpenBlock` frame while the `transform_scope_block` gate (`ctx.is_async && ctx.detect_for_await`)
stayed false for async generators, so `await using` lands in a frame stack that only
`dispose_resources(&func_env, …)` — which never looks at it — would drain. The PR is therefore
"suspend disposal in async generators **and** restore the disposal `main` dropped"; the group-B
slice below fixes it and must be the first thing that lands after the infrastructure.

**Coupling rule (why group B is one unit).** Once a block lowers to a scope frame, every way of
leaving it must dispose it: a `break`/`continue` gets `Goto` (`jump_terminator`, `generator_transform.rs`
~:468 — generators do not emit `LoopControl`), `reconcile_scope_stack` then truncates the frame
and its disposal is silently **dropped**, and a `throw`/`return`/`.return()`/`.throw()` crossing
it behaves the same. That is strictly worse than today's blocking-but-correct disposal of a
yield-free intact block. So the scope-state lowering, the static `break`/`continue` chain and
dynamic-crossing disposal are one landing unit: sub-commits (2a–2d) are fine, but the gate
`emit_scope_states` stays **off** until the last of them is green, and is flipped in the final
commit of the group. Stopping the PR anywhere inside group B means leaving the gate off.

### Group A — function-level disposal (independent, lands first)

1. **Infrastructure + normal completion (Completed terminator).**
   Test: `async-generator-await-using-fn-level-suspends-at-dispose.js` incl. the timer-backed
   disposer (`j.js`) and bare fall-off-the-end. Production: side table
   `generator_pending_dispose`, park/resume helpers, GC rooting, rewrite the Completed site
   (~:5809): `DisposeCursor::new(stack, Normal(undefined))`; on `Await` park (set
   `async_gen_yield_pending(true)`, request stays at queue front); on `Done(Normal)` resolve
   `{undefined, done:true}`, `pop_front`, `async_gen_process_queue`; on `Done(Throw)` mark completed
   + `reject_fn`; on `Done(Exit)` propagate the exit (today silently ignored). Also settles the
   "result promise one tick late" divergence (probes B/C2).
2. **`return;` / `return expr;` (Return terminator).** Tests:
   `async-generator-await-using-return-expr-awaits-before-dispose.js` (`ret.js`: disposer must
   start *after* `sync-end` and one tick later) **and** a resource-free tick-identity test for
   `return 5`, `return <native promise>` and `return <thenable>` (each with an empty dispose stack:
   settle position relative to a witness chain must be identical before/after the change — this is
   where a stray second Await from `async_generator_await_return` would hide). Production: for
   `return expr`, `Await(exprValue)` first (park with a return-operand-await step; a rejection is a
   throw completion at the return statement — `route_exception!` then dispose(Throw), same as the
   existing `Operand::Throw` arm), then dispose(`Return(v')`) suspendably, then settle **directly**
   with `{v', done:true}` (no second `PromiseResolve`, per `sec-asyncgeneratorstart`). Bare `return;`
   skips the Await. Keep the existing `finally`-present path (`pending_return`) untouched.
3. **Throw paths.** Test: `async-generator-await-using-throw-disposes-then-rejects.js` (body
   throw, `throw` statement, operand throw, ConditionalGoto/SwitchDispatch throws; the request
   rejects only after the async disposer finishes, `SuppressedError` chaining when the disposer also
   throws, a catch inside the generator still wins). Production: sites 4265/4840/5169/5235/5384
   park `dispose(Throw)` (`DisposeThen::Throw` -> mark completed -> reject -> queue advance) and the
   top-of-loop `.throw()` at a yield (~:4075).
4. **`.return(v)` at a yield (function-level resources).** Test:
   `async-generator-await-using-return-at-yield-disposes.js`, asserting only ordering-independent
   facts (disposer runs exactly once, after the yield-side code and before the request settles,
   request settles only after the disposer's promise settles, value is `v`). Do **not** assert the
   current tick offsets of the `Await(v)` (§8). Production: site ~:4175 (`pending_return`, no
   `finally` left) parks `dispose(Return(v))` then settles.
5. **`for (await using x of …)` in async generators.** Test:
   `async-generator-await-using-for-of-head-dispose-tick-alignment.js` (mirror of
   `await-using-for-of-head-dispose-tick-alignment.js`). Production: ForOfHead iteration-env
   disposal (~:5538) parks like `DisposeThen::ForOfIteration` (async-function precedent in
   `eval.rs` `for_of_stack`/`iteration_env`); iterator-close on abrupt exits stays as is.

### Group B — scope states for `await using` blocks/clauses (one landing unit; restores `n.js`)

Sub-commits, gate `emit_scope_states` off until 2d is green:
- **2a. Arms + normal exit.** `EnterScope` (push `ScopeFrame` onto `generator_scope_stacks`, child of
  the innermost env as in `reconcile_scope_stack`) and `ExitScope` (pop, `take_dispose_stack`, park
  with `DisposeThen::ScopeExit(after_state)`) replace the `unreachable!` at ~:5934. Rust-level
  check via the flag forced on in a unit test, or a `test262-extra` test run with the flag on.
- **2b. Static `break`/`continue` chain.** When a jump leaves await-using scope frames (compare
  `scope_depth` at the site with `LoopControlTarget.scope_depth`; track depths of frames that own
  `await using` separately from plain `OpenBlock` frames), the transform emits a chain of
  dedicated states — each carrying that frame's own `scope_depth`, terminator `ExitScope` —
  ending in `Goto(target)`. Order relative to intervening `finally`/for-of closes must stay
  innermost-first (read how gen `break` crosses `try/finally` today and hook in the same place).
- **2c. Dynamic crossing.** Before entering a catch/`finally` handler at try-depth *d* — via
  `route_generator_exception`, the top-of-loop `pending_return`/`pending_exception` unwind (`.return()`
  /`.throw()` while suspended at a `yield` inside a scope), and in-body `return` — dispose every
  await-using frame with `try_depth > d` innermost-first, parking with `DisposeThen::ScopeCrossReturn`
  / `ScopeCrossThrow` (analogue of `unwind_scopes_to!` in `async_function_resume`). Reuse the
  existing `DisposeThen` payload conventions.
- **2d. Transform gate + flip.** `emit_scope_states` (new flag; **do not overload
  `detect_for_await`**, it means "plain async function": `LoopControl` emission :469, the
  for-await/simple-machine short-circuit :550-553, `:559`; audit each of its ~14 reads) is set for
  async generators; the `Statement::Block` async-generator branch (intact block) is deleted and
  `transform_yielding_statement`/`transform_try_statement` route through `transform_scope_block` /
  `transform_clause_body`; `stmt_has_suspension` and the simple-machine short-circuit consult
  `has_suspendable_await_using_block` for async generators (this lowers `if`/loop/try containers
  whose only "suspension" is a nested `await using` block). `AwaitUsingScan::Blocked` fallbacks
  (`for(let…)`, `for-in`, `with`, lexical decl beside an isolatable block) stay on the tree-walker +
  blocking driver, as for async functions after #683.

Tests (write before 2a; they fail until the flip): `async-generator-await-using-block-yield-inside.js`
(`blk.js`: disposed once, after the yield resumes, no replay, `post` after `d`),
`…-try-list-disposes-before-finally.js` (`n.js`; assert `d` before `f`, the spec order),
`…-block-dispose-tick-alignment.js` (yield-free block; witness chain),
`…-nested-blocks.js`, `…-block-inner-await-suspends.js`, and
`…-block-abrupt-exit-routing.js` (mirror of `await-using-block-abrupt-exit-routing.js`: labelled and
unlabelled `break`/`continue` out of a scope in a loop with/without yields, `return` inside a scope
with and without an enclosing `try/finally`, throw inside a scope caught outside it,
`.return(v)`/`.throw(e)` while suspended at a `yield` inside a scope, scope inside `for-of` and
vice versa — one level each way per the ADR's stated boundary — nested scopes disposing
innermost-first, disposer rejection chaining).

### Group C

6. **Docs + cleanup.** CONTEXT.md and the ADR (§3); file the follow-up issues (§8) with
   `gh issue create`, label `needs-triage`.

**Cut line.** Group A is independently valuable and shippable. Group B must land whole (see the
coupling rule) or not at all; if it cannot be finished the gate stays off, the try-list regression
stays open, and the PR says `Refs #686` with the bisect result (`557f67da` -> `a129a725`) called
out as the remaining defect. `Closes #686` only if A and B are both complete. Commit + push after
each green slice (checkpoint rule). PR title (squash subject) must be Conventional Commits, e.g.
`fix(generators): suspend await-using disposal in async generators`.

## 5. Test surface

test262 targeted runs (`uv run python scripts/run-test262.py <dir>`), all must stay at 0 regressions
vs `origin/main:test262-pass.txt`:
- `test262/test/language/statements/await-using/` and `…/statements/using/` (65 files in the first;
  incl. the `…-asyncgeneratorbody`, `…-end-of-block`, for-of/for tests).
- `test262/test/language/statements/async-generator/`, `…/expressions/async-generator/`,
  `…/statements/class/**/async-gen-*` and `…/expressions/class/**/async-gen-*`,
  `test262/test/built-ins/AsyncGeneratorPrototype/`, `…/AsyncGeneratorFunction/`,
  `…/AsyncFromSyncIteratorPrototype/`, `…/for-await-of/`, `…/for-of/`.
- `test262/test/built-ins/AsyncDisposableStack/` and `DisposableStack/` (shared `DisposeCursor`).
- `test262/test/language/module-code/` await-using cases (shared driver code paths).
- Then the full default run (`language/`, `built-ins/`, `annexB/`, `intl402/`) before the PR.

Not covered by test262 -> new `test262-extra/` files (follow the header pattern of
`await-using-fn-level-suspends-at-dispose.js`: spec clause + algorithm excerpt in the `info:`
block, `flags: [async]`, `includes: [asyncHelpers.js, compareArray.js]`,
`features: [explicit-resource-management, async-iteration]`), one per behaviour, each citing the
clause it pins (`sec-asyncgeneratorstart`, `sec-return-statement-runtime-semantics-evaluation`,
`sec-block-runtime-semantics-evaluation`, `sec-disposeresources` (ERM)):
`async-generator-await-using-fn-level-suspends-at-dispose.js` (lead with the timer-backed disposer
— an observable *value* bug), `…-return-expr-awaits-before-dispose.js`,
`…-throw-disposes-then-rejects.js`, `…-return-at-yield-disposes.js`,
`…-for-of-head-dispose-tick-alignment.js`, `…-block-yield-inside.js`,
`…-block-dispose-tick-alignment.js`, `…-try-list-disposes-before-finally.js`,
`…-block-abrupt-exit-routing.js`, and `async-generator-await-using-dispose-suspended-gc-rooting.js`
(mirror `await-using-dispose-suspended-gc-rooting.js`: `$262.gc()` while the disposer's promise is
pending; the parked cursor's resources/dispose methods **and** the in-flight request's
`promise`/`resolve_fn`/`reject_fn` must survive — the existing `asyncGenAwaitFulfill` closures do
not root them, so the new park must).
`tests/` (host-compat, not spec): a `__host_exit` inside an async disposer of an async generator
must exit immediately with the code (mirror the issue-#242 disposal tests; today one site swallows it
and one `unreachable!`s). Rust unit tests in `dispose.rs` only if a new cursor step kind is added.

Gates: `cargo build --release -j4`; `cargo test --release`; `uv run python scripts/run-custom-tests.py`;
`./scripts/lint.sh` — as **separate** commands, never `&&`-chained. Never rebuild `target/release/jsse`
while a full test262 run is in flight (snapshot the binary first).

## 6. Regression risk

- **Baseline** (`test262-pass.txt`, read from `origin/main`; do **not** run `--update-baseline`):
  the shared risk is every async generator, because group A rewrites tails that *every* async
  generator completion goes through (Completed/Return/Throw/top-of-loop). The tick-count of
  `return expr` and normal completion with an **empty** dispose stack must be bit-identical —
  make "no resources => same path/same ticks" an explicit early branch and test it. Group B only
  affects async generators containing an `await using` block/clause (gated by
  `block_has_await_using`/`has_suspendable_await_using_block`), i.e. tiny in test262 terms;
  `language/statements/async-generator/` etc. are the canary for group A.
- **Shared machinery**: `DisposeCursor`/`dispose.rs` (also used by `AsyncDisposableStack.disposeAsync`
  and the async-function driver — do not change cursor semantics, add variants only);
  `route_generator_exception`, `unwind_generator_for_of_loops`, `reconcile_scope_stack`
  (exec.rs:1964) and `generator_scope_stacks` (#703 — sync generators use them too: any change to
  `reconcile_scope_stack` must keep `test262-extra/generator-loop-and-block-per-iteration-environments.js`
  and the async-function-*-scope tests green); the request queue + `async_gen_yield_pending`
  global flag (a park must set it, the resume must clear it and advance the queue exactly once);
  `StateTerminator` exhaustive matches (the sync-generator driver's `unreachable!` for
  `EnterScope`/`ExitScope` must stay unreachable: only emit them when the transform is for an async
  generator or async function).
- **GC**: new roots in `collect_gc_roots` + `free_gc_object`; run the rooting test under
  `$262.gc()` and a `cargo test --release` pass. `ObjectKind`/`IteratorState` are untouched
  (side table), so no exhaustive-match fallout.
- **Bytecode fast path / tree-walker hot paths**: not touched (`eval_expr`/`exec_statement`
  unchanged). Bytecode compiles function bodies only; async generators are state-machine
  transformed first, so no interaction expected — confirm with `bytecode_enabled` off (default).
- **Node-compat library harnesses**: none use `await using`; async-generator-heavy libs (e.g.
  luxon/zod streams if any) are only exposed through group A. Run `./scripts/run-library-tests.sh
  acorn` as a cheap smoke check only if group A code changes `Completed`/`Return` for
  resource-free generators.
- **Exit-code paths**: fixing the swallowed/`unreachable!` `Completion::Exit` sites must not
  change behaviour when no disposer calls `__host_exit`.

## 7. Implementation-stage checklist

- Start: `git status`, `git log -1` (expect `a129a725` + this plan commit), `cargo build --release -j4`.
- `git rm PLAN.md` in the first implementation commit that lands (the plan must not reach `main`).
- Scratch files under `$TMPDIR`, never `/tmp`. No `sudo`, no `pkill -9 <pattern>`.
- Commit each green slice; push after each green slice, at the latest after group A (checkpoint rule). PR body:
  the §1 table before/after, the design deviations from the issue text, the cut-line outcome.
- If a test262 test looks wrong, say so in the PR; do not bend the engine.

## 8. Out of scope (do not bundle) / follow-ups to file

- `.return(v)` at a yield awaiting `v` **before** unwinding (`sec-asyncgeneratorunwrapyieldresumption`):
  jsse unwinds first and awaits after via `async_generator_await_return`; separable from disposal
  (group A slice 4 tests deliberately avoid asserting those tick offsets).
- Probe **I** (`.return(v)` on a suspended-start/completed generator drains inline in
  `async_generator_await_return`) and probe **M** (`AsyncGeneratorAwaitReturn` does not keep the
  queue blocked; `it.return(pending)` then `it.next()` settle out of order) — no `await using`
  involved.
- Probe **K2**: `.return(v)` while parked in `yield*` with a function-level `await using` never runs
  the disposer (delegation return paths mark completed without `dispose_resources`) — file
  separately unless group A slice 4 turns out to share the code path.
- Remaining blocking `await_value`s in the async-generator driver that are not disposal
  (yield*/delegated `.next/.throw/.return`, inline `Completion::Yield` operand, `for await` step;
  see #687) and the many "settle then `drain_microtasks`" tails.
- `AwaitUsingScan::Blocked` shapes (`for (let…)` heads with `await using` bodies needing per-iteration
  bookkeeping, `for-in`, `with`, lexical decl beside an isolatable block) stay on the blocking driver
  (#684/#685 territory).
- Fully general interleaved unwind for `scope -> for-of -> scope` (three alternating levels; the
  ADR's stated boundary).
- Deleting the dead `block_exits`/`BlockExits` remnants in the async-function driver and
  `suspendable_dispose_block`/`parked_block_dispose` if they become unused — a cleanup PR, not this
  bug fix; the CONTEXT.md correction in §3 is docs only.
- Any formatting/refactor of `generator_runtime.rs` (6.8k lines), including de-duplicating the
  19 `StateMachineAsyncGenerator { … }` reconstruction blocks.
