# Plan: issue #646 — switch dispatch inside a generator's `try` bypasses `catch`/`finally`

## 1. Problem restated

When a `switch` whose body contains a `yield`/`await` is lowered by `generator_transform.rs`
into a `StateTerminator::SwitchDispatch`, the state-machine drivers evaluate the discriminant and
each `case` test with a bare `match self.eval_expr(..)`. On `Completion::Throw(e)` they finish the
generator (sync: `return Completion::Throw(e)`; async: reject the request promise) instead of
calling `route_exception!` → `route_generator_exception`, which is what walks `try_stack` to find
the enclosing `catch`/`finally`. So a throw from the discriminant or a case test escapes the
generator without running an enclosing `catch` or `finally`. Two drivers are affected:

- sync: `generator_next_state_machine_impl`, `src/interpreter/eval/generator_runtime.rs`
  `StateTerminator::SwitchDispatch` arm (~1434–1481; discriminant ~1440, case tests ~1457);
- async: `async_generator_next_state_machine_impl`, same file, arm at ~5270–5336 (discriminant
  ~5276, case tests ~5300).

Neighbouring `ForOfInit` arms (~1484 / ~5338) already do it right and are the model to copy.

**Hazard the issue's "fix shape" glosses over.** `route_exception!` expands to
`Completion::Empty => continue`. The `continue` must target the driver's outer, *unlabeled*
`loop { ... }`. The case-test evaluation sits inside `for case in cases { ... }`; a naive
`route_exception!(e)` there would `continue` the *inner `for`*, keeping the handler's
`current_id`/`pending_exception` that `route_generator_exception` just set but then evaluating the
next case test and possibly overwriting `current_id = case.state` — a silent wrong-state bug, not a
compile error. To avoid two hand-written shapes, *both* the discriminant and case-test sites funnel
to a single `route_exception!` call placed after a labeled block (§3). The in-repo precedent is
`async_function_resume`'s `SwitchDispatch` arm (`src/interpreter/eval.rs` ~8956–8992): a case-test
throw sets the pending exception, `matched = true; break;`s out of the `for`, and one
`if pending_exception.is_some() { continue; }` after the loop routes it. The `'dispatch:` block
below is that structure adapted to the macro-based generator drivers.

## 2. Spec basis

All from `spec/spec.html` (tc39/ecma262 at the pinned submodule commit):

- **§14.12.4 Runtime Semantics: Evaluation** (`sec-switch-statement-runtime-semantics-evaluation`),
  `SwitchStatement : switch ( Expression ) CaseBlock`: steps 1–2 `? Evaluation of Expression` /
  `? GetValue` — the discriminant's abrupt completion is returned from the statement; step 7
  `Completion(CaseBlockEvaluation ...)` returns `R` (abrupt propagates).
- **§14.12.3 CaseClauseIsSelected** (`sec-runtime-semantics-caseclauseisselected`): steps 2–3
  `? Evaluation of the Expression of C` / `? GetValue` — a throwing case test is an abrupt
  completion of the whole switch. **§14.12.2 CaseBlockEvaluation**
  (`sec-runtime-semantics-caseblockevaluation`) propagates it with `?` and stops evaluating
  further case tests (observable ordering — tested).
- **§14.15.3 Try Statement Runtime Semantics: Evaluation**
  (`sec-try-statement-runtime-semantics-evaluation`) and **CatchClauseEvaluation**
  (`sec-runtime-semantics-catchclauseevaluation`): a throw completion from the `Block` (which
  includes a nested `switch` statement) is delivered to `catch`; `Finally` always runs, and its
  normal completion re-instates `B`/`C` (the original throw).
- Generators: **GeneratorStart** (`sec-generatorstart`) and **AsyncGeneratorStart** (`sec-asyncgeneratorstart`) — an uncaught throw completes
  the generator (subsequent `next()` returns `{value: undefined, done: true}`) and, for async,
  rejects the pending request promise.

No new semantics; this is conforming an implementation gap to existing clauses.

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the two `SwitchDispatch` arms only.
- `test262-extra/generator-switch-abrupt-completions-through-try.js` (new).
- `test262-extra/async-generator-switch-abrupt-completions-through-try.js` (new).
- No `docs/`, `CONTEXT.md`, or ADR change (no new architecture or vocabulary).
- `test262-pass.txt` is **not** touched (baseline rolls forward only on `main`).
- `src/interpreter/eval.rs` (`async_function_resume`, `SwitchDispatch` ~8956) — **read only, no
  change**: that driver stores the throw in `pending_exception` and `continue`s to its loop-top
  router (~8535), so async *functions* are already correct.

Production-change shape (same in both drivers; keep the two drivers' existing handling of
non-Normal/non-Throw "other" completions byte-for-byte — they differ: sync `return other`, async
substitutes the `Yield` value/`undefined` — so **do not** factor a shared helper in this PR):

```rust
StateTerminator::SwitchDispatch { .. } => {
    let target: Result<usize, JsValue> = 'dispatch: {
        let disc_val = match self.eval_expr(discriminant, &term_env) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => break 'dispatch Err(e),
            other => /* existing sync/async handling, unchanged */,
        };
        for case in cases {
            let case_val = match self.eval_expr(&case.test, &term_env) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => break 'dispatch Err(e),
                other => /* existing handling, unchanged */,
            };
            if strict_equality(&disc_val, &case_val) { break 'dispatch Ok(case.state); }
        }
        Ok(default_state.unwrap_or(*after_state))
    };
    match target {
        Ok(state) => current_id = state,
        Err(e) => {
            let e = route_exception!(e);          // `continue`s the OUTER loop when handled
            /* existing "complete generator, return Throw / reject promise" cleanup, unchanged */
        }
    }
}
```

`route_exception!` is thus invoked outside the `for`, so its `continue` hits the driver loop. The
unhandled tail keeps the current cleanup (sync: mark `completed_state_machine_generator`, return
`Completion::Throw(e)`; async: `generator_inline_iters.remove`, mark completed async generator,
`reject_fn`, `drain_microtasks`, return `Completion::Normal(promise)`), mirroring the adjacent
`ForOfInit` arms exactly.

## 4. TDD slices

Preflight (once): `git submodule update --init --depth 1 test262` (spec already initialised);
`cargo build --release -j4` with an explicit long timeout (≥ 600 s) — do not rebuild while a
test262 run is in flight. Sanity-check expected outputs against `node` while authoring tests
(spec decides; node is only a cross-check). Create a task list (TaskCreate) from these slices
before starting.

1. **Red/green: sync generator, discriminant throws, caught.**
   Test: `test262-extra/generator-switch-abrupt-completions-through-try.js`, scenario
   `try { switch (thrower()) { case 1: yield 'z'; break; } } catch (e) { yield 'caught:' + e.message; } yield 'after';`
   → `next()` = `{caught:boom, done:false}`, then `after`, then `done`. Red today (throws out of
   `next()`). Production: restructure the sync `SwitchDispatch` arm per §3 (labeled block +
   `route_exception!` after it). Run the file, confirm green.
2. **Sync: case test throws, caught; evaluation order.** Same file.
   `switch (0) { case log('a'): case thrower(): case log('never'): yield 'z' }` inside `try/catch`:
   assert `a` logged, `never` not (§14.12.2 stops at the abrupt completion), catch runs, generator
   then continues. This slice guards the `continue`-in-`for` hazard: the asserted yield sequence
   fails if the route resumes the inner `for`. Production: already covered by slice 1's structure
   (case-test throw → `break 'dispatch Err(e)`); the test is red before the fix (the throw escapes
   `next()`, so the `catch` never runs) and the `never`-not-evaluated assertion guards the hazard.
3. **Sync: `finally` and unhandled paths.** Same file.
   (a) `try { switch (thrower()) { case 1: yield } } finally { log('cleanup') }` → `next()` throws
   `boom`, `cleanup` logged *before* it escapes, afterwards `next()` = `{undefined, done:true}`.
   (b) both `catch` and `finally`: catch handles, finally runs after, ordering asserted.
   (c) no enclosing `try`: throw escapes `next()` and generator is completed (current behaviour,
   must not regress). Variant with an enclosing `for-of` and *no* `try`
   (`for (x of it) { switch (thrower()) { case 1: yield } }`): the throw still escapes `next()`,
   and `it.return()` is now called exactly once — see §6, this is the one intended behaviour
   change on a path with no `try`.
4. **Sync: enclosing `for-of` is unwound by the route (load-bearing for §6).** Same file.
   `try { for (x of iterable) { switch (thrower()) { case 1: yield } } } catch (e) {..}`: the
   iterable's `return()` is called exactly once before the catch body runs (exercises
   `unwind_generator_for_of_loops` via `route_generator_exception`, shared with `ForOfInit`).
5. **Red/green: async generator, discriminant + case test + finally + unhandled.**
   Test: `test262-extra/async-generator-switch-abrupt-completions-through-try.js`
   (`flags: [async]`, `includes: [compareArray.js]`, chain with `.then(...).then($DONE, $DONE)`).
   Scenarios mirror 1–3: `next()` resolves `{value:'caught:boom', done:false}` instead of
   rejecting; `finally` runs before the rejection; unhandled case rejects and the generator is then
   completed. Production: same restructure in the async `SwitchDispatch` arm.
6. **Refactor/verify.** `cargo fmt`, `./scripts/lint.sh`, plain `cargo test --release` (per
   fmt-hook note, lib+bin), rerun both new files, then the broader runs in §5. Acceptance: run
   the three snippets from the issue body verbatim (sync `catch` → `{"value":"caught:boom","done":false}`;
   sync `finally` → `cleanup-ran` then `escaped: boom`; async `catch` →
   `resolved:{"value":"caught:boom","done":false}`) and match the Node outputs stated there.
7. **Sibling probe (no production change in this PR).** Using the built binary, probe
   `ConditionalGoto` (sync arm ~1330, async ~5173: `if/while/for` conditions containing no yield
   but with a yield in the body, inside `try`): by code reading they have the same bare
   `return Completion::Throw(e)` / reject bypass. If reproduced, open a follow-up issue with
   `gh issue create` and link it in the PR body rather than bundling.

Test-file conventions: test262 frontmatter (`description`, `esid` =
`sec-switch-statement-runtime-semantics-evaluation` with `info:` citing
`sec-runtime-semantics-caseclauseisselected` and `sec-try-statement-runtime-semantics-evaluation`,
`features: [generators]` / `[async-iteration]`); use `Test262Error` and `assert.sameValue` /
`assert.compareArray`; no comments beyond the frontmatter unless non-obvious.

## 5. Test surface

Targeted test262 runs (all currently 100% for generators; must stay so):

- `uv run python scripts/run-test262.py test262-extra/` (new files + the rest of the directory)
- `test262/test/language/statements/{switch,try,generators,async-generator,for-of,for-await-of}/`
- `test262/test/language/expressions/{generators,async-generator,yield,await}/`
- `test262/test/language/statements/class/` (generator/async-generator methods) and
  `test262/test/language/expressions/class/` (`gen-method`, `async-gen-method`)
- `test262/test/annexB/language/` subset touching switch function declarations
- Finally the full suite (`uv run python scripts/run-test262.py`) as CLAUDE.md requires after
  implementation work; compare against `origin/main:test262-pass.txt` (the runner's default
  baseline) — expect zero regressions and no baseline edits.

Not covered by test262 (hence `test262-extra/`): throw from switch discriminant/case test through
enclosing `try/catch/finally` when the switch body contains `yield`, for sync and async generators,
including case-test evaluation order, `finally` ordering, for-of unwinding, and post-throw
generator completion. Spec clauses named in the frontmatter as above.

`tests/` is not needed: nothing here is a host-compatibility or resource-limit check.

## 6. Regression risk

- **Baseline (`test262-pass.txt`):** low. Only the abrupt (Throw) path of `SwitchDispatch` changes;
  the Normal path selects `current_id` exactly as before. An escaping throw with no enclosing `try`
  still ends as `Completion::Throw` / promise rejection via the unchanged cleanup tail.
- **Intended behaviour change with no `try` involved (the likeliest baseline mover):** with no
  handler, `route_generator_exception` uses `keep_len = 0` and runs `unwind_generator_for_of_loops`
  over the whole `for_of_stack`, so a throwing switch dispatch inside a `for-of` now closes the
  loop's iterator (`return()` called) before the throw escapes. This is correct per ForIn/OfBodyEvaluation
  (`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`: an abrupt
  body completion performs IteratorClose) and matches the `ForOfInit` sites, but it was previously
  skipped. If a `for-of`/`for-await-of`/generator test262 test moves in either direction, inspect
  it against that clause before touching anything else. Slices 3(c) and 4 pin it.
- **Shared machinery leaned on:** `route_generator_exception` / `unwind_generator_for_of_loops`
  (may run user `iterator.return()` code — same exposure as `ForOfInit`), `try_stack` /
  `pending_exception` handling at `EnterCatch`/`EnterFinally`, `generator_inline_iters` /
  `generator_for_of_stacks` bookkeeping. No new GC roots: the thrown value is held in a Rust local
  across the route exactly as at the `ForOfInit` sites; `gc_safepoint()` behaviour is unchanged.
- **Not touched:** tree-walker hot paths (`eval_expr`/`exec_statement`), property MOP, exhaustive
  `ObjectKind` matches, bytecode fast path (off by default; generators use the state machine), the
  `async_function_resume` driver, Node-compat library harnesses. Libraries using generators
  (`uglify-js`, `acorn`) are unaffected on the non-throwing path; no library run planned beyond
  what CI does.
- **Main correctness risk:** the `continue`-in-`for` misroute (§1) — mitigated by routing outside
  the `for` and by slice 2's ordering assertion. Second risk: accidentally normalising the sync/async
  difference in "other" completions — mitigated by leaving those arms untouched.

## 7. Out of scope

- A shared `SwitchDispatch` helper across the sync/async drivers (their `other =>` handling
  differs; a dedup belongs in a separate refactor).
- Fixing `ConditionalGoto` (and any other terminator with the same bare-throw pattern) — probe in
  slice 7, follow-up issue if reproduced.
- The degraded inline-yield fallback for `case (yield x):` tests (issue #625 territory: case tests
  are not hoisted by `transform_switch_statement`, unlike the discriminant).
- Any change to `async_function_resume`, the bytecode compiler, `spec/`, `test262/`, or
  `test262-pass.txt`; formatting or unrelated cleanups.

PR title (squash subject): `fix(generators): route switch discriminant/case-test throws through enclosing try`
