# Plan: issue #663 — condition throw at a `ConditionalGoto` bypasses catch/finally in generators

## 1. Problem restated

In a generator or async generator compiled to a `GeneratorStateMachine`, an
`if` / `while` / `do-while` / `for` (or a `?:`, `||`, `&&`, `??` expression) whose
*test contains no suspension point* but whose body/branches do is lowered to a
`StateTerminator::ConditionalGoto { condition, .. }`. The driver evaluates
`condition` with a bare `match`; on `Completion::Throw(e)` it marks the generator
completed and returns the throw (sync) or rejects the request promise (async)
without calling `route_exception!` → `route_generator_exception`. An enclosing
`catch` / `finally` therefore never runs, `using` disposal is skipped, and
for-of iterators opened inside the `try` are not closed. The sibling
`SwitchDispatch` had the identical defect and was fixed in #664 (#646); this
issue is the same fix for `ConditionalGoto`, plus an audit of the other
terminators for the same bare-throw pattern.

Sites (`src/interpreter/eval/generator_runtime.rs`):
- sync `generator_next_state_machine_impl`, `StateTerminator::ConditionalGoto` (~L1330)
- async `async_generator_next_state_machine_impl`, `StateTerminator::ConditionalGoto` (~L5167)

### Empirical baseline (release build of this branch vs `node`)

Beyond the issue's repro, these all ESCAPE / reject in jsse but are caught in
node, and all funnel through the same `ConditionalGoto` arm:
`if` with `else`, `if` after a prior `yield`, labeled `if`, `do { yield } while (thrower())`,
nested `try/finally` inside `try/catch` (inner `finally` never runs),
`thrower() ? (yield 1) : 2`, `thrower() || (yield 1)`, `thrower() && (yield 1)`,
`thrower() ?? (yield 1)`, `while (thrower()) { yield }` under `try/finally`
(`finally` body skipped). Sync `for (let i=0;i<2;thrower())` update and `for (let i=thrower();…)`
init already work (they go through the yielding-statement path / state body).

### Audit of the other terminators (sync = `generator_next_state_machine_impl`, async = `async_generator_next_state_machine_impl`)

| Terminator | sync | async | Verdict |
|---|---|---|---|
| `ConditionalGoto` | bare throw | bare throw (and `Exit` swallowed into `undefined`) | **fix** (this issue) |
| `Return(expr)` | bare `dispose_resources` (~L1198), but **unreachable for throws**: yield-free `return e` stays a body statement (`transform_statements` only emits a `Return` terminator for async or for suspending exprs → `Identifier(temp)`), so throws route via the `stmt_result` path. Confirmed: `try { yield 0; return thrower() } catch` works in jsse. | bare throw (~L4771): `async function* f(){ try { yield 0; return thrower(); } catch(e){…} }` **rejects** in jsse, caught in node | **fix async only** (same pattern, verified red) |
| `Throw(expr)` | routes | routes | ok |
| `Yield` value eval | routes | routes | ok |
| `TryExit`, `SwitchDispatch`, `ForOfInit`, `ForOfHead` | route | route | ok |
| `Await` value eval | n/a | routes via `pending_exception` | ok |
| `Yield` `is_delegate` (`yield*`) `get_iterator` / `next` errors | catch-only ad hoc handling; `finally` skipped (`try { yield* 5 } finally {…}` never runs the finally) | reject bare | **out of scope** (different shape, own follow-up) |
| async `return <rejecting promise>` Await rejection | n/a | escapes `catch` | **out of scope** (follow-up) |
| async arms `other => yv/UNDEFINED` swallowing `Completion::Exit` (`Yield` value, `Throw`, `Await`) | — | swallow `__host_exit` | out of scope except in arms this PR rewrites |

`for (const x in …)` with `yield` in the body yielding nothing is already
tracked as #670 and is unrelated.

## 2. Spec basis

Clauses are cited by their stable `sec-…` ids in `spec/spec.html` (all verified present):

- `sec-if-statement-runtime-semantics-evaluation` (IfStatement Evaluation) — `? Evaluation of Expression`.
- `sec-runtime-semantics-dowhileloopevaluation`, `sec-runtime-semantics-whileloopevaluation`, `sec-runtime-semantics-forloopevaluation` + `sec-forbodyevaluation` — the test expression is evaluated with `?`, so a throw completion propagates out of the statement.
- `sec-conditional-operator-runtime-semantics-evaluation`, `sec-binary-logical-operators-runtime-semantics-evaluation` (covers `&&`, `||`, `??`) — same `?` propagation for the expression-level `ConditionalGoto` sites.
- `sec-try-statement-runtime-semantics-evaluation` (TryStatement Evaluation) — a throw completion from the `try` Block is handed to `CatchClauseEvaluation`; the `Finally` block is always evaluated (`B`, then `F`).
- `sec-return-statement-runtime-semantics-evaluation` (ReturnStatement Evaluation) — `? Evaluation of Expression` / `? GetValue` precedes `Await` in async generators (the async `Return` arm).
- `sec-generatorstart` / `sec-asyncgeneratorstart` — a throw completion that leaves the body runs `DisposeResources` of function-level `using` declarations, then the generator completes (sync: the throw propagates to the `next()` caller; async: the request promise rejects via `AsyncGeneratorCompleteStep`, `sec-asyncgeneratorcompletestep`). Existing code comments cite these as §27.5.3.3 / §27.6.3.3; keep that wording in code comments for consistency.
- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset` — a throw completion inside a `for-of` body runs `IteratorClose` before propagating (`route_generator_exception` already unwinds loops; the routing must keep that).

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs`
  - sync `ConditionalGoto` arm (~L1330).
  - async `ConditionalGoto` arm (~L5167).
  - async `Return` arm throw/exit handling (~L4762–4797).
- `src/interpreter/tests.rs` — `node_host_tests`: `__host_exit` regression for async `ConditionalGoto` (pattern: `host_exit_in_async_generator_switch_dispatch_is_not_swallowed`, ~L3759).
- New: `test262-extra/generator-conditional-goto-abrupt-completions-through-try.js`
- New: `test262-extra/async-generator-conditional-goto-abrupt-completions-through-try.js`
- New (or folded into the async file above): `test262-extra/async-generator-return-expression-throw-through-try.js`
- No `docs/`, `CONTEXT.md`, or ADR change: no new architecture or vocabulary. `generator_transform.rs` is **not** touched — the transform is correct; only the driver mishandles the completion.

## 4. TDD slices

Model every step on the merged #664 fix (`a86201e9`, `git show a86201e9`): same file layout,
same `route_exception!` usage, same test file shape.

**Rules for the edit:** `route_exception!` expands to a `continue` on the driver `loop`, so it
must sit directly in the `match &terminator` arm (or in a `match` arm inside it), never inside a
nested `for`/`loop`/closure. Keep the `Completion::Exit` path exactly as `__host_exit` (#242)
requires: uncatchable, no routing, no disposal, no promise settlement.

1. **RED — sync conditions** (`test262-extra/generator-conditional-goto-abrupt-completions-through-try.js`).
   Run with `cargo build --release` then
   `./target/release/jsse test262-extra/generator-conditional-goto-abrupt-completions-through-try.js`
   (or `uv run python scripts/run-test262.py test262-extra/generator-conditional-goto-abrupt-completions-through-try.js`).
   Cases, each asserting node-equal results with `assert.compareArray` / `assert.sameValue`:
   `if`, `if/else`, labeled `if`, `while`, `do-while`, `for(;test;)` caught by `catch`;
   `if` under `try/finally` (finally runs once, then the throw escapes `next()`);
   nested `try/finally` inside `try/catch` (inner finally runs before outer catch);
   `?:`, `||`, `&&`, `??` with a throwing left/test and a `yield` in the other operand;
   a condition throw *after* a prior `yield` (resume path, not first entry);
   condition throw inside a `for-of` body inside `try/catch` → the iterator's `return()` is called before the catch runs;
   unhandled condition throw with a function-level `using` → disposer runs before the throw escapes, generator is `done` afterwards (`next()` → `{value: undefined, done: true}`);
   a caught-then-continued generator (`catch` yields, then more code) to prove state survives routing.
   Header: `esid: sec-if-statement-runtime-semantics-evaluation`, `features: [generators, explicit-resource-management]`, `includes: [compareArray.js]`; `info:` cites the `?` in the Evaluation steps and `TryStatement` Evaluation.
   Expected red: every catch/finally case fails today.
2. **GREEN — sync `ConditionalGoto`**. Replace the `Completion::Throw(e)` arm body with the same tail as sync `SwitchDispatch`
   (~L1461): `let e = route_exception!(e);` → `dispose_resources(&func_env, Completion::Throw(e))` (§27.5.3.3) →
   `completed_state_machine_generator(...)` → `self.generator_inline_iters.remove(&o.id)` → `return disp`.
   Keep `other => return other` for non-throw abrupt completions (already returns `Exit`).
3. **RED — async conditions** (`test262-extra/async-generator-conditional-goto-abrupt-completions-through-try.js`, `flags: [async]`, `features: [async-iteration, explicit-resource-management]`).
   Same case matrix as slice 1 using an async `collect`/`rejection` helper like the #664 async file; add a case where the condition throw is caught and the generator then continues to `await`. Assert the request promise rejects (not resolves) in the unhandled cases and that the disposer ran first.
4. **GREEN — async `ConditionalGoto`**. Rewrite the `eval_expr(condition)` match like async `SwitchDispatch` (~L5264):
   - `Throw(e)` → `route_exception!(e)`; then `dispose_resources` (`Throw` → continue, `Exit(code)` → `return Completion::Exit(code)`, else `unreachable!`), `generator_inline_iters.remove`, mark `completed_state_machine_async_generator`, `reject_fn`, `drain_microtasks()`, `return Completion::Normal(promise)`.
   - `Exit` → `discard_generator_for_of_loops_on_exit`, mark completed, `return exit` (currently swallowed into `undefined`).
   - `Yield(yv)` / other → keep existing `yv` / `UNDEFINED` fallback.
5. **RED→GREEN — `__host_exit` in conditions** (`src/interpreter/tests.rs`, `node_host_tests`).
   Add `host_exit_in_async_generator_conditional_goto_is_not_swallowed` (looped over `if (__host_exit(3)) { yield 1 }`, `while (__host_exit(4)) { yield 1 }`, `for (;__host_exit(5);) { yield 1 }`; assert `interp.pending_exit == Some(code)`, sentinel global not advanced, `Completion::Exit(code)`), and the sync twin `host_exit_in_generator_conditional_goto_…` (green already; pins the contract). Red for async before slice 4's `Exit` arm.
6. **RED→GREEN — async `Return` expression throw** (`test262-extra/async-generator-return-expression-throw-through-try.js`, or a section of the slice-3 file).
   `async function* f(){ try { yield 0; return thrower(); } catch(e){ yield 'caught:'+e.message } }`, plus the `finally` variant, plus function-level `using`. Red today (rejects). Fix the `Completion::Throw(err)` arm (~L4771): `let err = route_exception!(err);` before the existing `dispose_resources`/reject tail; make the `other` arm propagate `Exit` instead of swallowing it. Add the sync equivalent to slice 1's file as a pin (green today; documents why the sync `Return` terminator arm needs no change). **Do not** touch the sync `Return` arm.
   Do **not** try to fix `return <rejecting promise>` here (see Out of scope).
7. **Refactor pass (only if the diff shows duplication)**: none planned. Do not extract a shared helper for the sync/async tails; #664 kept them inline and the arms differ in settlement.
8. **Full gate** — see §5.

## 5. Test surface

- New `test262-extra/` files above (sync, async, async-return), run with
  `uv run python scripts/run-test262.py test262-extra/` (no dedicated runner; pass the directory). They follow the test262 header pattern (`description`, `esid`, `info`, `includes`, `features`, `flags`) and cite the clauses in §2.
- Targeted test262 runs (must not regress; run before and after):
  - `test262/test/language/statements/if/`, `while/`, `do-while/`, `for/`, `try/`
  - `test262/test/language/expressions/generators/`, `async-generator/`, `conditional/`, `logical-and/`, `logical-or/`, `coalesce/`
  - `test262/test/language/statements/generators/`, `async-generator/`, `for-of/`, `for-await-of/`, `switch/`
  - `test262/test/built-ins/GeneratorPrototype/`, `AsyncGeneratorPrototype/`
  - `test262/test/language/statements/using/`, `await-using/` (disposal ordering touched by the new `dispose_resources` call)
- `cargo test --release` (includes the new `node_host_tests` and `tests/test262_smoke_oracle.rs`), run as its own command.
- `./scripts/lint.sh` (its own command; rustfmt + clippy `-D warnings` also run via the edit hook).
- `uv run python scripts/run-custom-tests.py` for `tests/`.
- Full `uv run python scripts/run-test262.py` at the end; the baseline comes from `origin/main:test262-pass.txt`, do not pass `--update-baseline`.
- Environment: `spec/` and `test262/` are empty in a fresh workspace; `git submodule update --init --depth 1 test262` (and `spec`) first. Build with a capped `-j` and a scratch `CARGO_TARGET_DIR` under `$TMPDIR`; never rebuild while a suite run is in flight.

## 6. Regression risk

- **Baseline movement:** none expected in a downward direction; a throw that previously escaped a generator now goes to an enclosing handler only where the spec says it must. Small upward movement is possible for any test262 test that relied on this.
- **Shared machinery leaned on:** `route_generator_exception` / `unwind_generator_for_of_loops` (for-of `IteratorClose`, per-iteration `using` disposal, `try_stack` truncation); `dispose_resources` (§27.5.3.3 / §27.6.3.3); `discard_generator_for_of_loops_on_exit`. Not touched: tree-walker hot paths (`eval_expr`/`exec_statement`), `property.rs` MOP, GC rooting (`gc_safepoint`; `route_generator_exception` already keeps `for_of_stack` synced via `sync_generator_for_of_stack`), `ObjectKind` matches, bytecode fast path.
- **Behavior change to watch:** a condition throw now calls `dispose_resources` in sync (previously skipped). Correct per §27.5.3.3 and consistent with sync `SwitchDispatch`/`Throw`.
- **Async `Exit`:** turning the swallowed `Exit` into a propagated exit changes only `__host_exit` inside a condition; covered by the new Rust test.
- **Library harnesses:** generators/async generators are heavily used by bundled libraries. Sanity-run one generator-heavy library after the change (e.g. `./scripts/run-library-tests.sh acorn`, ~minutes) — optional, not gating.

## 7. Out of scope (file follow-ups, do not bundle)

- `yield*` delegate errors (`get_iterator`, `next` lookup, `next()` call) in sync and async generators: catch-only ad hoc routing, `finally` skipped (`try { yield* 5 } finally { … }` in jsse never runs the finally; node does).
- Async `return <rejecting promise>` inside `try`: the `Await(exprValue)` rejection escapes the `catch` (node catches).
- Async arms that swallow `Completion::Exit` via `other => yv/UNDEFINED` (`Yield` value eval, `Throw`, `Await`).
- for-in with `yield` in the body yielding nothing (already #670).
- Any restructuring of the `InlineYield`/`generator_context` fallback (#625), deduplicating the sync/async terminator tails, or a shared "finish generator with throw" helper.
- Editing `generator_transform.rs`, `test262-pass.txt`, `spec/`, `test262/`.

PR title suggestion (squash subject): `fix(generators): route if/while/for/conditional test throws through enclosing try`
