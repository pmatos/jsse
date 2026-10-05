# Plan: issue #842 — Annex B function hoisting lost once a block/loop body is lowered to a state machine

## 1. Problem restated

When a sloppy-mode function body containing a `function` declaration nested inside a
block/loop (an Annex B §B.3.3-eligible declaration) is lowered to a generator/async
state machine — which happens for *any* generator with a `yield`, and for any async
function/async generator that needs to suspend, independent of `await using` — the var
binding `FunctionDeclarationInstantiation` is supposed to create for that name at
function entry is never created. Reading the name from the function/script scope
afterward (`typeof g`) observes `undefined` instead of the function, because the
tree-walking Annex B pass (`Interpreter::collect_annexb_function_names` +
the registration logic in `instantiate_body_declarations`, `src/interpreter/exec.rs`)
never runs over the function's *original* body at the point the state machine is
constructed — it only runs against whatever fragment of the (already-split) body a
given state happens to hold, and the fragment that actually contains the nested
`function` declaration always lives one scope level deeper than the function's own var
scope, where the existing Annex B gate (`!is_block_scope`) is false by construction. No
per-container fix inside the lowering (`transform_scope_block`'s `EnterScope`/
`ExitScope`, or the generic `ScopeAction::OpenBlock` path in `generator_transform.rs`)
can recover this, because Annex B hoisting is a function/script-scope-wide decision
made once, at entry, over the whole body — not a per-block concern.

## 2. Spec basis

- **`sec-functiondeclarationinstantiation`** (`spec/spec.html:13719`, `oldids:
  sec-web-compat-functiondeclarationinstantiation` — the former Annex B.3.3.1 text is
  now folded directly into ordinary `FunctionDeclarationInstantiation`):
  - Step `step-functiondeclarationinstantiation-web-compat-insertion-point`
    (`spec/spec.html:13830-13838`): "If the host is a web browser or otherwise supports
    [...], for each `FunctionDeclaration` *f* directly contained in the `StatementList`
    of any `Block`, `CaseClause`, or `DefaultClause` [...], if replacing *f* with a
    `VariableStatement` would not produce an early error and *parameterNames* does not
    contain *F*, create an `undefined`-initialized mutable binding for *F* in *varEnv*"
    — this is the step that is currently skipped for state-machine-lowered bodies. It
    is gated on `strict` being `false` (sloppy mode only).
  - Step `step-functiondeclarationinstantiation-alt-funcdecl-eval`
    (`spec/spec.html:13839-13844`): the runtime mirror ("when the `FunctionDeclaration`
    *f* is evaluated [...] copy *bEnv*'s binding value for *F* into *fEnv*"). This half
    is **already implemented correctly** and is untouched by this fix
    (`src/interpreter/exec.rs:1207-1221`, the `Statement::FunctionDeclaration` arm of
    `exec_statement`, which reads `var_scope.annexb_function_names` to decide whether
    to mirror the value up). It keeps working for lowered bodies today because
    `Statement::FunctionDeclaration` nodes execute through the same `exec_statement`
    regardless of whether they're reached via the tree-walker or a state-machine
    dispatch — only the *entry-time binding creation* half is missing.
  - `sec-block-level-function-declarations-web-legacy-compatibility-semantics`: the
    named Annex B clause the `normative-optional` condition in
    `sec-functiondeclarationinstantiation` refers to; jsse implements this
    unconditionally (not gated on a "web browser host" flag), matching its existing
    sloppy-mode behavior.
  - `sec-runtime-semantics-evaluategeneratorbody` (`spec/spec.html:24167`, `oldids:
    sec-generator-function-definitions-runtime-semantics-evaluatebody`) and
    `sec-runtime-semantics-evaluateasyncfunctionbody` (`spec/spec.html:25409`, `oldids:
    sec-async-function-definitions-EvaluateBody`): both run step 1 = `Perform ?
    FunctionDeclarationInstantiation(functionObject, argumentsList)` **before**
    `GeneratorStart`/`AsyncFunctionStart` — i.e. synchronously at call time, before the
    function object (or its driving state machine) ever suspends. This is why the fix
    belongs at state-machine *construction* time (when jsse builds the
    `GeneratorStateMachine` and sets up its initial environment), not inside the
    resume/dispatch loop that runs afterward.
  - Async generators follow the equivalent `AsyncGeneratorStart`
    (`sec-asyncgeneratorstart`, `spec/spec.html:50645`), same ordering.

## 3. Files to touch

- `src/interpreter/exec.rs` — extract the existing Annex B registration logic (current
  inline block in `instantiate_body_declarations`, roughly lines 269-358) into a new
  shared method, e.g. `pub(crate) fn register_annexb_function_names(&mut self, stmts:
  &[Statement], env: &EnvRef, is_global: bool, all_annexb: Vec<String>)`, and change its
  final write from an overwrite to a union-merge with any `annexb_function_names`
  already on `env` (see Regression risk below for why). `instantiate_body_declarations`
  calls it unchanged in behavior for the tree-walking path.
- `src/interpreter/eval.rs` — three call sites that build a `GeneratorStateMachine` at
  function-entry time, each gets one new call to the shared helper, scanning the
  **original, pre-lowering** `body.as_slice()` (not anything from the
  `GeneratorStateMachine`/`GeneratorState::body` fragments), targeting the same
  environment that already receives `state_machine.local_vars`/`temp_vars` at that site:
  - `call_async_function` (plain async function, non-generator): target `func_env`
    (there is no separate `varEnv` split out for non-simple parameter lists in this
    path at all — see Regression risk).
  - the `is_async_generator` branch inside `call_function`: target `exec_env` (which is
    `body_env` when parameters are non-simple, else `func_env`).
  - the `is_generator` branch inside `call_function`: target `exec_env`, same rule.
  - `execute_async_module` (`src/interpreter/mod.rs:3965`) is explicitly **not**
    touched: modules are always strict, and the web-compat step is gated on `strict`
    being `false`.
- `test262-extra/` — three new test files (see §5).
- No `docs/adr/` entry: this is a bug fix restoring already-decided, already-specified
  behavior to a code path that was missing it, not a new architectural decision.
- No `CONTEXT.md` change: no new vocabulary.

## 4. TDD slices

1. **Extract `register_annexb_function_names`, make it merge instead of overwrite.**
   Pure refactor of `src/interpreter/exec.rs`: move the inline block out of
   `instantiate_body_declarations` into the new method, with the final
   `var_scope.borrow_mut().annexb_function_names = Some(registered)` changed to merge
   (union, de-duplicated) with whatever names, if any, are already recorded on `env`,
   instead of replacing them. `instantiate_body_declarations` keeps calling it exactly
   where the inline block used to sit, with `env` as the target (it already equals
   `var_scope` at that call site — the method takes `env` directly and drops the
   now-redundant intermediate name). No new test: behavior for every *existing* caller
   is unchanged, because `Environment::reset_function_scope` already resets
   `annexb_function_names` to `None` before any environment is reused, and today's only
   caller invokes this exactly once per non-block-scope entry, so there is nothing to
   merge against yet. Verify with `cargo test --release` and a targeted run of
   `test262/test/annexB/` (see §5) to confirm zero behavior change on the one path that
   already runs this code.
2. **Async function: fix + regression test.** Add
   `test262-extra/async-function-state-machine-preserves-annexb-function-hoisting.js`
   reproducing the issue (bare block / `while` body / `for (var ...)` body, each with a
   bare `await 0` to force lowering) and confirm it fails against the current binary.
   Then add the call to `register_annexb_function_names` in `call_async_function`
   (`src/interpreter/eval.rs`, after the existing `local_vars` loop, before
   `scheduler.insert_async_function_state`), gated on `!is_strict`, targeting
   `func_env`, scanning `body.as_slice()`. Re-run the new test to confirm it passes.
3. **Plain generator: fix + regression test.** Add
   `test262-extra/generator-state-machine-preserves-annexb-function-hoisting.js` (sync,
   using `yield` instead of `await` to force lowering, driven with plain `.next()`
   calls — no `asyncHelpers.js` needed). Confirm it fails first. Add the matching call
   in the `is_generator` branch of `call_function`, after its `local_vars` loop, before
   `IteratorState::StateMachineGenerator { ... }` is constructed, targeting `exec_env`,
   scanning `body.as_slice()`.
4. **Async generator: fix + regression test.** Add
   `test262-extra/async-generator-state-machine-preserves-annexb-function-hoisting.js`
   (`flags: [async, noStrict]`, `includes: [asyncHelpers.js]`, driven with `for await`
   or manual `.next()` promise unwrapping). Confirm it fails first. Add the matching
   call in the `is_async_generator` branch of `call_function`, after its `local_vars`
   loop, before `IteratorState::StateMachineAsyncGenerator { ... }` is constructed,
   targeting `exec_env`, scanning `body.as_slice()`.

Each of slices 2-4 is independently red/green and touches exactly one call site plus
one new test file; slice 1 is the shared foundation they all build on. All four stay
inside this one PR (they're one coherent fix, not independent features), but are
structured so each can be reviewed and bisected on its own commit.

## 5. Test surface

**New `test262-extra/` files** (no test262 coverage exists for this intersection — see
below), one per function kind, each `esid: sec-functiondeclarationinstantiation`,
`flags: [noStrict]` (plus `[async]` and `includes: [asyncHelpers.js]` for the async
cases), following the structure of the existing
`test262-extra/await-using-block-preserves-annexb-function-hoisting.js`:

- `async-function-state-machine-preserves-annexb-function-hoisting.js`
- `generator-state-machine-preserves-annexb-function-hoisting.js`
- `async-generator-state-machine-preserves-annexb-function-hoisting.js`

Each file must assert, per function kind, at minimum (using a **distinct function name
per shape** — `g`/`h`/`j`/... as in the existing `await-using-block-...` test — never
reusing one name across shapes, since a shared name would let fixing any single shape
make all the others' assertions pass too):
- Bare block: `{ function g(){} <suspend>; }` then `typeof g === 'function'`.
- `while` body: `while (cond) { function h(){} <suspend>; }` then `typeof h ===
  'function'`.
- `for (var ...)` body: `for (var i = 0; i < 1; i++) { function j(){} <suspend>; }`
  then `typeof j === 'function'`.
- **Init-order assertion**: reference the bound name directly (not `typeof`, which
  can't distinguish an unbound name from one bound to `undefined`) *before* the block
  that declares it has run: `assert.sameValue(k, undefined)` ahead of `{ function
  k(){} <suspend>; }`. Without the fix this throws a `ReferenceError` (no binding
  exists yet); with the fix it reads `undefined`, proving the entry-time step
  (`step-functiondeclarationinstantiation-web-compat-insertion-point`) actually ran,
  not merely that the value got copied up later when the block executed.
- **Mixed inline + split block**, to exercise the merge behavior from §6: one block
  with no suspension point before the function's first `await`/`yield` (`{ function
  a(){} }`, which stays inside the depth-0 fragment, unsplit) followed later by a
  split block (`{ function b(){} <suspend>; }`). Assert both `typeof a === 'function'`
  and `typeof b === 'function'`. This is the one case that would catch a clobber if
  the merge-vs-overwrite change in slice 1 is implemented wrong.
- A parameter-shadow case (simple parameter list only — see Regression risk):
  `function f(g) { { function g(){} } return g; }`-shaped — the declaration must be
  skipped (`parameterNames` already contains `F`), so the parameter's own value must
  survive untouched.
- A top-level `let`/lexical conflict case: a top-level `let g` must block the Annex B
  binding for a same-named nested `function g(){}` entirely.
- The `arguments` name, modeled on the existing (already-passing, tree-walker-only)
  `test262/test/annexB/language/function-code/block-decl-func-skip-arguments.js`:
  wrap its expectations in each lowered shape. This is a guard (confirming the
  tree-walker and the state-machine path keep agreeing), not a red/green test for this
  issue — the spec text for where exactly the `F !== "arguments"` check applies reads
  ambiguously against this `alt-funcdecl-eval` revision, so test262's existing,
  already-passing behavior is the authority to match, not a fresh reading of the
  prose.
- A `"use strict"` control case confirming nothing is hoisted in strict mode (the gate
  stays `!is_strict`).

Every expected value must be cross-checked against `node` before being written down
(per project authority order), especially the parameter-shadow and lexical-conflict
cases, which are easy to get backwards.

**Existing test262 directories to run as regression gates** (none of these currently
combine Annex B with `async`/generator lowering, so they validate "didn't break
anything," not "fix works"):
- `test262/test/annexB/language/function-code/` (65 tests under
  `sec-web-compat-functiondeclarationinstantiation` total across test262; this
  directory is the bulk of them, entirely synchronous).
- `test262/test/annexB/language/statements/function/`
- `test262/test/language/statements/async-function/`
- `test262/test/language/expressions/async-function/`
- `test262/test/built-ins/AsyncFunction/`
- `test262/test/language/statements/generators/`
- `test262/test/language/expressions/generators/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/expressions/async-generator/`
- `test262-extra/` in full (includes the `#665`-era
  `await-using-block-preserves-annexb-function-hoisting.js`, which must keep passing
  unchanged, and the three new files above).
- `uv run python scripts/run-test262.py` (full suite) before opening the PR, per
  project convention — not to move `test262-pass.txt` (that stays untouched, read from
  `origin/main`), but to confirm no regression against the existing baseline.
- `cargo test --release` (covers `generator_analysis.rs`/`generator_transform.rs` unit
  tests, which are untouched by this fix but exercise adjacent code).
- **The actual blocking CI gate for this change, reproduced locally before opening the
  PR** — `ci.yml` runs `test262-extra/` three extra ways beyond a plain release run,
  and all three block merges, so the three new files must be run through each:
  `--bytecode` (the bytecode-compiler fast path), `JSSE_GC_STRESS=7` (forces a
  collection at nearly every safepoint — see `AGENTS.md`'s GC Stress Mode section), and
  the `release-checked` binary (`cargo build --profile release-checked`, which enables
  the root-stack balance `debug_assert!`s). Concretely:
  `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --bytecode`,
  `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/`, and
  `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/`.
  Do not assume these constructs fall back to the tree-walker under `--bytecode`
  without checking — confirm by actually running the new files through it, since an
  unsupported-construct bail there is a `CompileError::Unsupported` guess, not a
  verified fact.

## 6. Regression risk

- **Shared hot path**: `instantiate_body_declarations` is the var/function/lexical
  declaration-instantiation entry point for every function, script, and eval body in
  the tree-walker — the single riskiest piece of shared machinery this change touches.
  The extraction in slice 1 must be behavior-preserving; the merge-vs-overwrite change
  is the one semantic difference, and it is a no-op for every *current* caller (see
  below), so the full `annexB` + function/generator/async-function test262 regression
  run in slice 1 is the gate that catches anything missed.
- **Why the merge-not-overwrite change matters, and what actually protects the fix
  today without it:** `exec_state_machine_body` (`src/interpreter/exec.rs:26`) routes
  every per-state fragment of a lowered body through `exec_body_inner` →
  `exec_statements_cached(fragment_stmts, env, None)` → `instantiate_body_declarations`,
  which re-runs its own (uncached, `analysis: None`) Annex B collection against *that
  fragment's own statement list* every time a state dispatches. Not every block
  containing an Annex-B-eligible declaration is split off the depth-0 fragment: a block
  with no suspension point before the function's first `await`/`yield` (e.g. `{
  function a(){} }` appearing *before* the first `await`) never gets its own
  `EnterScope`/`OpenBlock` state — it stays inlined in state 0's own fragment, at
  `scope_depth` 0, where `is_block_scope` is `false` and the Annex B section *does*
  run. So the per-fragment collection genuinely finds `a` again on that rerun. What
  stops that rerun from clobbering the entry-time registration today is the `is_param`
  heuristic: by the time state 0 dispatches, the entry-time pass (this fix) has already
  created `a`'s binding directly in `func_env`/`exec_env`; that binding is not one of
  the fragment's own *top-level* var/function declarations (`a` is nested inside a
  `Block`, not a direct statement of the fragment), so the `is_param` check
  (`env.borrow().bindings.contains_key(&name) && !top_level_var_names.contains(&name)`)
  treats the existing binding as if it were a parameter conflict and skips
  re-registering it — `registered` comes out empty for `a`, and (pre-merge-fix) the
  destructive overwrite at the end of the block never fires for that name specifically.
  That protection is real today, but it's an accidental side effect of a heuristic
  designed for an unrelated purpose (excluding actual formal parameters), not a
  guarantee independent of it — any future change that makes a depth-0 fragment
  re-discover a name through some path other than "already in `env.bindings`" would
  have nothing stopping a destructive overwrite without the merge change. Making the
  final write a union closes that off unconditionally instead of relying on the
  `is_param` side effect continuing to hold by accident. The mixed inline/split test in
  §5 (`a` inline, `b` split) is what actually exercises this today — both must pass
  before and after the merge change, since neither is expected to change behavior; the
  merge change is the backstop for if the `is_param` protection ever stops applying.
- **Pre-existing, out-of-scope gap this fix inherits (do not fix here, do note in the
  PR)**: `call_async_function` (plain, non-generator async functions) never splits out
  a separate `varEnv` for non-simple parameter lists the way the ordinary-function
  path (`call_function`'s non-generator branch) and the generator/async-generator
  paths do — it reuses a single `func_env` for everything regardless of
  `has_simple_params`, and never sets `has_simple_params = false` on it either. That
  means the Annex B "skip if a non-simple-param function's name matches a parent
  binding" rule (the `!env.borrow().has_simple_params` branch inside the shared
  helper) can never fire for plain async functions, matching its pre-existing (already
  wrong, already out of scope) behavior for ordinary var-hoisting in that same case.
  To avoid entangling this fix with that unrelated gap, every new test262-extra test
  case uses parameterless or simple-parameter-only async functions.
- **Known pre-existing tree-walker quirk, not touched**: a nested `var x` sharing a
  name with a block `function x(){}` can skip Annex B registration via the `is_param`
  heuristic (`env.borrow().bindings.contains_key(&name) && !top_level_var_names`)
  treating the pre-existing non-top-level var binding as if it were a parameter. This
  predates this issue, applies equally to the tree-walker and (after this fix) the
  state-machine path, and is out of scope.
- **`test262-pass.txt`**: not updated by this PR (read-only baseline operation reserved
  for `main`); the full-suite run in §5 is to confirm no regressions relative to the
  baseline, not to move it.
- **Bytecode VM**: the bytecode compiler/VM (`bytecode/`) is a separate, AST-level fast
  path; nothing in this fix touches it directly, but do not assume it is unaffected —
  confirm by actually running the three new `test262-extra/` files with `--bytecode`
  (see §5's CI-gate list) rather than assuming a `CompileError::Unsupported` bail falls
  back to the tree-walker for this shape.
- **GC rooting**: no new allocations/roots are introduced — `register_annexb_function_names`
  only mutates existing `Environment` bindings already reachable from the call sites'
  existing roots (`func_env`/`exec_env`), the same objects `local_vars`/`temp_vars`
  already write into at each of the three sites.

## 7. Out of scope

- Relaxing issue #665's `contains_annexb_function_declaration` guard in
  `generator_analysis.rs` (added so `await using` block/loop-body lowering doesn't
  newly hit this gap). Once this fix lands, that guard becomes conservative rather
  than load-bearing for correctness, and *could* be relaxed to let more containers with
  Annex-B-eligible declarations be lowered — but that's a separate, independently
  reviewable follow-up, not part of closing this bug.
- The separate `try { function g(){} } finally {}` bug mentioned in the issue body,
  which reproduces with no async/lowering involved at all and was filed as its own
  issue.
- The non-simple-parameter-list `varEnv` gap in `call_async_function` described above.
- The pre-existing nested-`var`-vs-block-`function` `is_param` heuristic quirk.
- Any refactor of the direct-`eval` Annex B block (`src/interpreter/eval.rs:6311-6392`,
  a structurally similar but textually separate piece of Annex B logic for
  `eval_declaration_instantiation`) to also call the new shared helper. It is a
  plausible follow-up dedup, not required to close this issue, and eval's
  declaration-instantiation rules differ enough (it mutates an *existing* outer
  var/global scope rather than creating one) that merging it in isn't a drop-in
  change.
- Any formatting-only or unrelated cleanup in the touched files.
