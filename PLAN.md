# Plan: issue #726 — `await` in catch-parameter / for-in/of-head / for-init destructuring defaults is evaluated by the tree-walker

## 0. Branch is stale — rebase before implementing

This worktree's branch point (`4b9789f8`) is behind `origin/main`
(`git rev-parse origin/main` at planning time: `b887b68b`; re-resolve at
implementation time, this repo moves fast) on exactly the files this issue
touches. In particular:

- **#744 / #727** (`fix(generators): suspend yield in var/let/const pattern defaults`)
  already built a strip-to-temp rewrite for catch-param and for-in/of-head
  patterns — `hoist_yield_pattern` in `generator_transform.rs`, called from
  `transform_try_statement` and `transform_for_in_of_loop`. It is gated on
  `pattern_contains_yield` only.
- **#760** (`fix(generators): preserve iteration across head-pattern yield`)
  extended detection of a `yield`-containing for-in/of head pattern so the loop
  uses the state machine and keeps its iterator live across suspension.
- Docs (`docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md`'s "not
  covered" list, `CONTEXT.md`'s **Pattern Lowering** glossary entry) were **not**
  updated when the above landed, and still describe catch/for-heads as entirely
  unlowered and `pattern_needs_lowering` as await-only — both now false. This
  plan corrects those too (§3).

**The implementation stage must `git fetch origin main` and rebase this branch
onto it before writing any code**, then re-verify every line reference below —
do not trust this branch's stale copy of `generator_transform.rs`/
`generator_analysis.rs`/`eval.rs` until the rebase lands.

## 1. Problem restated

`generator_transform.rs`'s `hoist_yield_pattern` already rewrites a catch
parameter or a `for`/`for-of`/`for-await-of` `Variable`-kind head pattern into a
trivial `Pattern::Identifier($tmp)` for the driver's single-call binding sites
(`EnterCatch`/`ForOfHead`), re-homing the real pattern as a synthesized
`let <pattern> = $tmp;` prepended to the catch/loop body — but only when the
pattern contains a `yield`. The top-level `contains_suspension` gate that
decides whether a `yield`-only pattern's *container* needs the compiled state
machine at all is **not symmetric between the two binding sites**:
`contains_suspension`'s `Statement::ForIn`/`ForOf` arms already flag a
`yield`-containing head pattern (`for_in_of_variable_head_contains_yield`,
added by #760, alongside a matching `analyze_statement`/
`analyze_pattern_expressions` update so the generator's yield-point count
agrees — see §3), so a for-in/of head's `yield` already forces the compiled
path and is already correctly lowered today. The `Statement::Try` arm, by
contrast, still does **not** check the catch parameter's pattern at all —
documented as a *deliberate* choice
(`docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md`,
"Post-review follow-up"): when a catch-param pattern's `yield` is a
generator's *only* suspension, it correctly stays on the tree-walker's
single-state "simple machine" path, where `bind_pattern`'s now-correct
`Completion::Yield` propagation reaches the generator runtime's `InlineYield`
fallback and suspends/resumes correctly — routing it onto the compiled state
machine instead would be pure overhead, not a fix. This plan's `await`-specific
widening (§3) touches all three arms (`Try`, `ForIn`, `ForOf`) uniformly —
additively, beside whatever each already does for `yield` — because `await`'s
tree-walker fallback is broken (blocking, not suspending) at every one of
these sites, unlike `yield`'s at the for-in/of-head site.

`await` has no such fallback: the tree-walker's `bind_pattern` reaches an
`await` default via `eval_expr` → `Expression::Await` → the blocking
`await_value`, which *drains the microtask queue inline* rather than truly
suspending — jobs scheduled before the `await` run after it (issue repro:
`w1,c5,w2,b6,w3,sync-end` instead of `sync-end,w1,c5,w2,b6,w3`). So the fix is
asymmetric with the already-landed `yield` work: `hoist_yield_pattern`'s trigger
needs widening to include `await`, **and** `contains_suspension`'s `Try`/
`ForIn`/`ForOf` arms need a new, `await`-specific check that intentionally does
*not* touch their existing (correct, tested) `yield` handling.

A third site, `for (var {a = await 1} = …;;)` initializers, is untouched by any
of the above — `contains_suspension`'s `Statement::For` arm and
`transform_for_statement`'s `ForInit::Variable` gate only ever look at a
declarator's `init` expression, never its `pattern`, for either `yield` or
`await`. Both ADRs above independently record this as still open. This plan
closes it for `await` only, matching the issue's scope.

**A precision this plan must not skip** (found while re-checking the existing
`yield` desugar, not in the original bug report): `transform_for_in_of_loop`
currently rewrites `left` to the stripped `$tmp` pattern *before* building
**both** `ForOfInit` and `ForOfHead` — so a `yield`-containing head pattern
already loses its real bound names from `ForOfInit`'s transform-time TDZ
pre-declaration (`for_of_head_tdz_env`) today, a narrow, documented, untested
residual gap. Widening the same shared code path to `await` without splitting
this would *newly* break a case that works correctly today (`await` never
triggers the rewrite pre-#726, so `ForOfInit.left` is always the real pattern
for `await`-only heads right now). §3/§4 fix this by splitting which `left`
feeds which terminator — a small, low-risk change that also happens to close
the pre-existing `yield` gap as a side effect, since both triggers share one
code path.

## 2. Spec basis

- `sec-runtime-semantics-catchclauseevaluation` (*CatchClauseEvaluation*):
  creates `catchEnv` (a declarative environment child of the running lexical
  environment), binds the catch parameter's names into it via
  `BindingInitialization`, then evaluates the catch block. `catchEnv` is what
  the driver's `EnterCatch` arm already builds (`eval.rs`, `catch_env`); this
  issue is about what runs *inside* that `BindingInitialization` step when it
  reaches an `await`.
- `sec-runtime-semantics-forinofheadevaluation` (*ForIn/OfHeadEvaluation*):
  declares the lexical head's bound names in TDZ, in a throwaway environment,
  *before* the iterable expression is evaluated
  (`ForOfInit`/`for_of_head_tdz_env`, which builds its TDZ layer purely from
  `left.bound_names()` — confirmed by reading `for_of_head_tdz_env` directly;
  it has no other use). `ForOfInit`'s `left` field is **not** even IC-cleared
  (`clear_terminator_ic_sites`'s own comment: "the `left` binding is applied
  later in ForOfHead, which is where its sites are cleared") — confirming
  `ForOfInit.left` exists solely to supply `BoundNames` for TDZ, never for
  binding or IC purposes, and can safely stay the *original* pattern while
  `ForOfHead.left` carries the *stripped* one.
- `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`
  (*ForIn/OfBodyEvaluation*), lexical-binding branch: each iteration creates a
  **fresh** `iterationEnv` (`NewDeclarativeEnvironment`), performs
  `ForDeclarationBindingInstantiation`, sets it active, then calls
  `ForDeclarationBindingInitialization`. This is exactly the environment the
  driver's `ForOfHead` arm builds (`bind_env` / `ForOfLoopState::iteration_env`
  / `effective_env()`) and where an `await` in the head pattern's default needs
  to suspend.
- `sec-forbodyevaluation` (C-style `for`) / `sec-createperiterationenvironment`:
  its per-iteration environment is unrelated to the two ForIn/Of clauses above.
  The C-style-for slice does not touch `CreatePerIterationEnvironment`'s
  machinery at all (`transform_for_statement`'s existing
  `per_iteration_bindings`/`CopyForward` handling, unchanged) — it only needs
  the existing declarator-lowering machinery to be *reached*, and that
  machinery already runs inside the per-iteration frame `transform_for_statement`
  sets up before emitting the init.
- `sec-runtime-semantics-keyedbindinginitialization` (*KeyedBindingInitialization*):
  what `lower_pattern_binding` already implements (landed for #709, reused
  verbatim by the `yield` desugar and by this plan). Not changed by this plan —
  only the trigger for routing catch/for-head/for-init patterns *into* it
  changes. **Caveat confirmed by reading `lower_pattern_binding` directly**: it
  emits one *separate* `Statement::Variable` per object-pattern property, in
  source order, rather than pre-declaring every bound name up front — an
  already-accepted imprecision (`docs/adr/2026-09-21-2143-...md`: "a
  `let`/`const` binding created mid-machine gets the same TDZ precision
  `SentValueBindingKind::Pattern` already gives"). A sibling reference
  (`catch ({a = b, b = await 1})` where an outer `b` exists) will read the
  *outer* `b` instead of throwing a TDZ `ReferenceError` — pre-existing since
  #709's plain-declaration form, not something this plan changes or is
  responsible for fixing; see §7.
- `ForInOfLeft::Pattern`/`ForInOfLeft::Expression` heads (bare assignment
  targets, e.g. `for ({a = await 1} of xs)`) bind via
  `sec-runtime-semantics-keyeddestructuringassignmentevaluation`, a different
  mechanism (assignment, not `BindingInitialization`) that inherits #724's
  blocker — see §7.

## 3. Files to touch

**Verified: no third detector (`analyze_generator_body`/`analyze_statement`/
`analyze_pattern_expressions`) needs touching for `await`.** #744's ADR gap #3
and #760 both had to widen `analyze_generator_body`'s yield-point collector
because `transform_generator_inner_opts`'s simple-machine shortcut
(`generator_transform.rs` ~lines 497–510) gates on `analysis.yield_points.is_empty()`
for the sync-generator case, and on `is_async && analysis.yield_points.is_empty()
&& !contains_suspension(body) && …` for the async case — i.e. `yield_points`
and `contains_suspension` are two independent, OR'd conditions for taking the
shortcut. Reading `analyze_expression`'s `Expression::Await` arm confirms it
pushes nothing to `analysis.yield_points` (unlike `Expression::Yield`, which
does) — it only recurses into its operand to find a *nested* `yield`. So
`yield_points` exists purely for generator `.next()`-resumption bookkeeping,
which `await` never participates in; for the async case, `!contains_suspension(body)`
alone gates the shortcut. Widening `contains_suspension` (below) is therefore
sufficient for `await` at all three sites — no change to
`analyze_generator_body`/`analyze_statement`/`analyze_pattern_expressions` is
needed or planned.

- `src/interpreter/generator_analysis.rs`:
  - New `pattern_needs_await_lowering`, mirroring the shape of the existing
    `pattern_needs_lowering` but restricted to `await`:
    ```rust
    pub(crate) fn pattern_needs_await_lowering(pattern: &Pattern) -> bool {
        pattern_contains_await(pattern) && pattern_lowering_supported(pattern, false)
    }
    ```
    **Every new check in this plan uses this, never a bare
    `pattern_contains_await`.** Gating on `pattern_lowering_supported` matters:
    an *unsupported*-shape pattern (array pattern, object rest beside a
    suspending sibling) containing `await` must **not** force the enclosing
    catch/for-head/for-init onto the compiled state machine — that would
    silently change `test_unsupported_pattern_shapes_stay_on_the_tree_walker`-style
    behavior (currently asserted only for plain declarations, but the same
    principle — "unlowerable shape stays on the tree-walker/single-state
    path") for these three new sites, for no benefit: the shape still can't be
    lowered, so routing it through a multi-state machine only adds overhead
    without fixing anything (that fix is #725's job, unchanged by this plan).
  - New `for_in_of_variable_head_contains_await`, mirroring
    `for_in_of_variable_head_contains_yield` (~line 850) but built on
    `pattern_needs_await_lowering`:
    ```rust
    fn for_in_of_variable_head_contains_await(left: &ForInOfLeft) -> bool {
        match left {
            ForInOfLeft::Variable(decl) => decl
                .declarations
                .iter()
                .any(|d| pattern_needs_await_lowering(&d.pattern)),
            ForInOfLeft::Pattern(_) | ForInOfLeft::Expression(_) => false,
        }
    }
    ```
  - `contains_suspension`'s `Statement::Try` arm (~line 1273): add a disjunct
    checking the catch parameter pattern with `pattern_needs_await_lowering`.
  - `contains_suspension`'s `Statement::ForIn`/`Statement::ForOf` arms (~lines
    1261–1269): OR in `for_in_of_variable_head_contains_await(&f.left)`
    alongside the existing `for_in_of_variable_head_contains_yield(&f.left)`
    call — additive, not replacing.
  - `contains_suspension`'s `Statement::For` arm (~line 1251): add a disjunct
    checking each `ForInit::Variable` declarator's pattern with
    `pattern_needs_await_lowering`.
  - Do **not** touch `contains_yield` — `await` is a `SyntaxError` outside
    async functions, so a sync generator body can never contain one.
  - Unit tests in the `tests` module for all new checks (mirroring the
    existing `contains_suspension_sees_awaiting_declaration_patterns` test),
    including one asserting an unsupported shape (`var [a = await 1] of xs`
    in a catch/for-head position) does *not* flip `contains_suspension`.
- `src/interpreter/generator_transform.rs`:
  - `hoist_yield_pattern` (~line 1800): widen the trigger from
    `pattern_contains_yield(pattern)` to `pattern_contains_yield(pattern) ||
    pattern_needs_await_lowering(pattern)`, and **rename** it (e.g.
    `hoist_suspending_pattern`) since it is no longer yield-specific — update
    its doc comment and both call sites (`transform_try_statement` ~line 2846,
    `transform_for_in_of_loop` ~line 2714). The rewrite logic itself
    (`Pattern::Identifier(temp)` + synthesized `let <pattern> = <temp>;`) is
    unchanged; only the gate widens.
  - `transform_for_in_of_loop` (~lines 2710–2756): **split `left`.** Currently
    `let left: &ForInOfLeft = rewritten_left.as_ref().unwrap_or(left);` is used
    for *both* the `ForOfInit` and `ForOfHead` terminators. Change so
    `ForOfInit`'s `left` field always gets the **original, un-rewritten**
    pattern (so `for_of_head_tdz_env`'s `BoundNames` walk keeps seeing the real
    names, for both `yield`- and `await`-triggered rewrites), and only
    `ForOfHead`'s `left` field gets the rewritten `$tmp` pattern when
    `hoist_suspending_pattern` fired. Concretely: keep the original `left: &ForInOfLeft`
    parameter binding unchanged for `ForOfInit`'s construction, and use the
    `rewritten_left.as_ref().unwrap_or(left)` value only where `ForOfHead` is
    built. This is a net *correctness fix* for the already-shipped `yield` path
    too (previously-shared, now split), not new behavior gated on `await`.
  - `transform_for_statement`'s `ForInit::Variable` gate (~lines 2528–2536):
    widen from `d.init.as_ref().is_some_and(|e| expr_has_suspension(e,
    ctx.is_async))` to also check `pattern_needs_await_lowering(&d.pattern)`
    per declarator — **`await`-only, using the same predicate as the
    `contains_suspension` gate**, not the general `pattern_needs_lowering`.
    Reason this must *not* be general: `analyze_generator_body`'s yield-point
    collector (`docs/adr/2026-09-22-1752-...md`, gap #3) still doesn't walk a
    for-init declarator's pattern, so if a sync/async generator is *already* on
    the compiled path for some other `yield` and this gate used the general
    (`yield`-inclusive) predicate, `transform_variable_declaration` would lower
    a `yield`-containing for-init pattern into real `Yield` terminator states
    that the yield-point collector never counted — a structural mismatch (wrong
    yield-point bookkeeping), not just a missed optimization. Keeping this gate
    `await`-only sidesteps that collector entirely, since it only applies to
    async functions/async generators' `await` handling, not the yield-point
    count.
  - No other change to `transform_try_statement`/`transform_for_in_of_loop`
    beyond the renamed/widened `hoist_suspending_pattern` call and the `left`
    split above.
  - `stmt_has_suspension`'s for-await guard (~line 658,
    `is_async && f.is_await && !for_in_of_left_contains_suspension(&f.left)`)
    is **already general** (`for_in_of_left_contains_suspension` checks both
    `await` and `yield`) — confirm empirically in slice 4 that no edit is
    needed here; only touch it if slice 4's test proves otherwise.
  - Unit tests in the `tests` module for the renamed/widened
    `hoist_suspending_pattern`, the `left`-split, and the
    `transform_for_statement` gate.
- `docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md` — "What this
  change does not cover": remove the "Catch parameters and for-in/of heads"
  bullet and the "`for (var {a = await 1} = …;;)` initializers" bullet (both
  now covered for `await`); note in passing that `yield` at these same sites
  was already covered by #744/#760 before this issue landed (the list was
  stale on this point), and record the `ForOfInit`/`ForOfHead` `left`-split
  fix.
- `docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md` — "Left as
  residual, not attempted here": update the `await` bullet to record that #726
  closed the gap by widening `hoist_yield_pattern`'s (now renamed) trigger and
  adding the `await`-specific `contains_suspension` checks; update the
  self-referential-TDZ residual bullet to record that it is now fixed (by the
  `left` split) for both triggers, not just inherited.
- `CONTEXT.md` — **Pattern Lowering** glossary entry currently says "only an
  `await` (not a `yield`) triggers it" (already inaccurate — `pattern_needs_lowering`
  covers both as of #744) and "catch parameters, for-in/of heads... are not
  lowered yet" (now false for both triggers). Correct both sentences and add
  one line on the strip-to-temp shape used at `EnterCatch`/`ForOfHead`/for-init
  sites.
- `test262-extra/*.js` — new regression tests (§5).

## 4. TDD slices

Each slice is red (new test fails on `origin/main`, post-rebase) → green
(production change makes it pass). Build with `cargo build --release -j4` (cap
parallelism); run new tests with `timeout`. Quality gates as **separate**
commands per CLAUDE.md: `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test --release`, `./scripts/lint.sh`.

1. **C-style `for` initializer — no interaction with the `left`-split or the
   yield-desugar machinery, smallest diff.**
   Tests (`test262-extra/async-function-for-init-destructuring-default-await-order.js`):
   - `for (var {a = await 1} = {};;)` (and `let`, `const`, with a `break` to
     terminate), asserting `sync-end` precedes the tick the default's `await`
     resolves on, mirroring the issue's own ordering assertion, with this as
     the function's *only* suspension (exercises the new `contains_suspension`
     `Statement::For` arm check, not just the inner lowering).
   - **Per-iteration environment, discriminating test**:
     `for (let {a = await 0} = {}; a < 2; a++) { fns.push(() => a); }` —
     expect `fns[0]()` to return `0` and `fns[1]()` to return `1`. A shared
     (non-per-iteration) environment would instead give `2, 2` for both, since
     `a` would be one mutable slot all closures alias; the per-iteration
     `CopyForward` frame (§2, `sec-createperiterationenvironment`) must give
     each closure its own snapshot. (A same-value-every-iteration test, e.g. a
     default that never changes, cannot distinguish the two — this is why the
     loop variable itself must be the thing captured and mutated.)
   Code: `contains_suspension`'s `Statement::For` arm gains
   `|| d.declarations.iter().any(|d| pattern_needs_await_lowering(&d.pattern))`
   (`generator_analysis.rs`); `transform_for_statement`'s `ForInit::Variable`
   gate gains `|| pattern_needs_await_lowering(&d.pattern)`
   (`generator_transform.rs`). `transform_variable_declaration`/
   `lower_pattern_binding` are untouched — this slice only has to get them
   invoked from inside the existing per-iteration-frame setup.

2. **Catch parameter.**
   Tests:
   - `test262-extra/async-function-catch-destructuring-default-await-order.js` —
     the issue's own `catch ({a = await 5})` repro in isolation (no for-of),
     asserting `sync-end` precedes the resumed tick.
   - `test262-extra/async-function-catch-destructuring-default-await-scope.js` —
     the catch parameter's bound names are visible inside the catch body and
     *not* visible after the `try`/`catch` (leak check into function scope),
     with a suspending default in the mix.
   - A rejecting default (`catch ({a = await Promise.reject(new Error("x"))})`)
     rejects the enclosing async function's own promise — assert via
     `f().then(() => { throw new Error('should have rejected') }, e => log.push('rejected:' + e.message))`,
     not by asserting an uncaught rejection — and a `finally` block after the
     `try`/`catch` still runs before that rejection propagates.
   Code: widen `hoist_yield_pattern`'s trigger (rename to
   `hoist_suspending_pattern`) to `pattern_contains_yield(pattern) ||
   pattern_needs_await_lowering(pattern)`; add the `Statement::Try` arm's
   `await`-only check to `contains_suspension`. No change to
   `transform_try_statement` beyond calling the renamed function — the
   `EnterCatch`/synthesized-`let` machinery already exists and is already
   tested for `yield` by the (main-only, not yet present on this branch)
   `generator-yield-in-catch-param-default.js`; this slice proves it also
   works when the trigger is `await`.
   **Not planned**: a sibling-TDZ test (`catch ({a = b, b = await 1})` with an
   outer `b`) — traced through `lower_pattern_binding` (§2) this is an
   already-accepted, pre-#726 imprecision shared with the plain-declaration
   form; adding a test for it here would be testing (and failing on)
   out-of-scope behavior. See §7.

3. **For-in/of head — `var`/`let`/`const`, plain (non-`await`-keyword) loop,
   the issue's own repro.**
   Tests:
   - `test262-extra/async-function-forof-destructuring-default-await-order.js` —
     the issue's `for (var {b = await 6} of [{}])` in isolation, for `var`,
     `let`, `const`.
   - `test262-extra/async-function-forin-destructuring-default-await-order.js` —
     same shape for `for-in` (`ForOfHead` is shared between for-in and for-of
     in this engine).
   - `test262-extra/async-function-forof-destructuring-default-await-per-iteration-env.js` —
     `for (let {a = await 1} of [{}, {}])` pushing a closure per iteration;
     after the loop each closure returns its *own* iteration's `a`.
   - `test262-extra/async-function-forof-destructuring-default-await-unwind.js` —
     `break`/`return` from the loop body on the iteration *after* the head's
     default has already suspended once, plus a rejecting default, asserting
     the source iterable's `return()` is called exactly once (instrument and
     count).
   - `test262-extra/async-function-forof-destructuring-default-await-tdz.js` —
     **shadowed self-reference**: `let a = 'outer'; ... for (let {a = await 1}
     of [a]) {}` must throw `ReferenceError`. (Not `for (let {a = await 1} of
     [a]) {}` with no outer `a` — that throws "not defined" regardless of
     which `left` `ForOfInit` sees, so it cannot discriminate a correct
     `left`-split from a broken shared one. The shadowed form only throws if
     `ForOfInit`'s TDZ environment is built from the *real* `a`, per §2/§3's
     `left`-split fix — verify this already passes on `origin/main` today,
     pre-rebase-target's baseline for `await` specifically, since `await`
     never triggers the rewrite before this plan lands.) Run it both as the
     function's only suspension and alongside an unrelated `await` elsewhere.
     Also add a **generator** variant covering the `yield`-side fix the
     `left`-split incidentally makes (§0/§1): `let a = 'outer'; function* g() {
     for (let {a = yield 1} of [a]) {} } var it = g(); it.next()` must throw
     `ReferenceError` on the first `.next()` call — today this silently reads
     the outer `'outer'` instead, since `yield`'s rewrite already shares the
     un-split `left`. This is the one place this plan's `await`-focused fix
     changes already-shipped `yield` behavior, so it earns its own explicit
     test rather than relying on the `await` test alone to cover it.
   Code: widen `hoist_suspending_pattern`'s trigger (shared with slice 2,
   already done there), add the `Statement::ForIn`/`Statement::ForOf` arms'
   `await`-only check to `contains_suspension`, and apply the `left`-split
   (§3) in `transform_for_in_of_loop`. For the `var` variant,
   `bind_pattern`'s `Pattern::Identifier` arm (`exec.rs`) takes the
   `BindingKind::Var` path (`Environment::find_var_scope`, walking to the
   function-scoped slot the function-entry `temp_vars` loop pre-declared)
   rather than creating a binding local to `bind_env` — this matches how an
   unlowered `for (var b of …)`'s single, reused, function-scoped `b` already
   behaves today; the lowering states that follow read `$tmp` by ordinary
   chain lookup from the same `bind_env` and see the same slot.

4. **For-await-of head, plain object-pattern default (only the lowerable
   shape).**
   Test: `test262-extra/async-function-for-await-of-destructuring-default-await-order.js` —
   `for await (let {a = await 1} of asyncIterableOf([{}]))` as the function's
   *only* suspension, asserting correct interleaving and that the mandatory
   per-step `Await(nextResult)` still happens exactly once per iteration
   (instrument the async iterable's `next()` call count).
   Code: slices 2–3's `contains_suspension`/`hoist_suspending_pattern`
   widening is already `is_await`-agnostic (same functions, same terminators,
   for both `for-of` and `for-await-of`), and `stmt_has_suspension`'s
   for-await guard already calls the general `for_in_of_left_contains_suspension`
   (not the yield-only helper) — so **write and run this test first against
   slices 2–3's code, with no further edit**, and confirm: the guard's
   `!for_in_of_left_contains_suspension(&f.left)` is already `false` here (the
   head does contain a suspension), so `stmt_contains_for_of_head` doesn't
   return early via that branch, but falls through to `contains_suspension(stmt)`,
   which now returns `true` via slice 3's widened `Statement::ForOf` arm
   regardless. If the test passes with no code change, **make no edit** —
   leave the existing guard as is. If it fails, the guard needs its own look
   (this predicate is shared with unrelated for-await logic, so a failure here
   would be a signal to re-examine assumptions, not a known fix to apply
   blind).

## 5. Test surface

No test262 coverage exists for this behavior — `test262/test/language/statements/try/dstr/`,
`test262/test/language/statements/for-of/dstr/`, `test262/test/language/statements/for-in/dstr/`
have no `await`-referencing files (these are generic dstr fixtures shared across
binding contexts), and `test262/test/language/statements/for-await-of/`'s
`await`-containing files are array-pattern (`dstr-array-elem-*`) fixtures, not
this issue's object-pattern-default-ordering shape. (Re-verify this after the
rebase — confirm no relevant test262 file landed between this planning run and
implementation.) All slices' tests belong in `test262-extra/`, following
`test262-extra/async-function-destructuring-default-await-suspends.js`'s
harness pattern from #709 (shared `log`/timestamp array,
`Promise.resolve().then()` chain markers, `setTimeout(() => print(log.join(",")), 0)`
at the end).

Targeted regression runs (must not move `test262-pass.txt`, read from
`origin/main`, so no `--update-baseline` on this branch):
- `uv run python scripts/run-test262.py test262/test/language/statements/try/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for-of/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for-in/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for-await-of/`
- `uv run python scripts/run-test262.py test262/test/language/statements/for/`
- `uv run python scripts/run-test262.py test262/test/language/statements/async-function/`
  and `test262/test/language/statements/async-generator/` (shared
  `contains_suspension`/`stmt_has_suspension` machinery)
- `uv run python scripts/run-custom-tests.py` (test262-extra + tests/) — **must
  include re-running `generator-yield-in-catch-param-default.js`,
  `generator-yield-in-for-in-of-head-default.js`, and their async-generator
  counterparts** (already on `main`, not yet on this branch pre-rebase). Two
  different things to confirm, not one: `generator-yield-in-catch-param-default.js`'s
  "sole construct stays on the tree-walker" case (only the `Try` arm has this
  property, per §1) must still take the single-state path after this plan's
  `await`-only `Try`-arm widening; `generator-yield-in-for-in-of-head-default.js`
  (already on the compiled path for `yield` via #760, not a "sole construct
  stays on tree-walker" case at all) must keep behaving the same way it does
  today, and additionally must not regress from the `left`-split (§3) — see
  §6.
- Full suite: `uv run python scripts/run-test262.py`, after the targeted runs
  are clean.

## 6. Regression risk

- **The one mistake this plan is structured to prevent: using a
  both-triggering predicate (bare `pattern_contains_await`,
  `pattern_needs_lowering`, or `pattern_contains_suspension`) instead of the
  shape-gated `pattern_needs_await_lowering` in the four `contains_suspension`
  arms, the `hoist_suspending_pattern` trigger, and the
  `transform_for_statement` gate.** Two distinct failure modes this avoids:
  (a) an unsupported-shape `await` pattern (array pattern, object rest)
  unnecessarily forcing a multi-state compiled machine where today's
  single-state path is correct and sufficient (#725 stays #725's job); (b) a
  `yield`-containing for-init pattern getting lowered into real `Yield`
  terminator states that `analyze_generator_body`'s yield-point collector
  never counted, desyncing the state machine's resume bookkeeping — this one
  is a `transform_for_statement`-specific risk (§3) since that function's gate
  was the one place this plan considered (and rejected) using the general
  `pattern_needs_lowering`.
- **The `ForOfInit`/`ForOfHead` `left`-split (§3) touches shared code that both
  `yield`- and `await`-triggered rewrites flow through, across all three
  state-machine drivers** (async function in `eval.rs`, sync generator and
  async generator in `eval/generator_runtime.rs` — `git grep -n "ForOfInit"`
  finds three `match` arms total). Verified safe by reading all three
  directly: every one calls `Self::for_of_head_tdz_env(left, &term_env)` and
  nothing else on `left`, and `clear_terminator_ic_sites` explicitly does
  *not* clear `ForOfInit.left`'s IC sites (only `ForOfHead.left`'s),
  confirming `ForOfInit.left` has exactly one consumer everywhere it's
  handled. The risk is entirely in slice 3's TDZ test (§4) catching a
  transcription mistake (e.g. swapping which terminator gets which `left`),
  not in an unknown second consumer.
- **Shared machinery.** `contains_suspension` and `stmt_has_suspension`/
  `transform_try_statement`/`transform_for_in_of_loop`/`transform_for_statement`
  are on the hot path for every `try`, `for`, `for-in`, `for-of`,
  `for-await-of`, and async function/async generator body during
  state-machine construction. A mistake here is not contained to
  destructuring. The targeted test262 directories in §5 exist specifically to
  catch that before the full suite runs.
- **`stmt_has_suspension`'s for-await guard** already uses the general
  `for_in_of_left_contains_suspension` — slice 4 is written to detect if that
  assumption is wrong empirically rather than to blindly apply a fix.
- **GC rooting / `gc_safepoint()`**: the new temp lives in `catch_env`/the
  for-of `iteration_env`, both already GC roots via `scope_stack`/
  `for_of_stack` (unchanged structures) — no new rooting path.
- **Bytecode fast path**: `bytecode_enabled` is off by default; not touched.
- **`ObjectKind` exhaustive matches**: unaffected — no new object kind.
- **Library harnesses**: none of the pinned libraries exercise `await` inside
  a catch/for-head/for-init destructuring default in their own source; not
  re-run as part of this fix, per existing practice.

## 7. Out of scope

- **`ForInOfLeft::Pattern`/`ForInOfLeft::Expression` for-of/for-in heads**
  (bare assignment targets: `for ({a = await 1} of xs)`) — bind via
  `KeyedDestructuringAssignmentEvaluation`/`assign_to_for_pattern`, not
  `BindingInitialization`, and inherit #724's blocker. Not this issue's job.
- **Sibling-reference TDZ imprecision inside a single lowered pattern**
  (`catch ({a = b, b = await 1})` reading an outer `b` instead of throwing) —
  traced to `lower_pattern_binding` emitting one `Statement::Variable` per
  property rather than pre-declaring all bound names up front
  (`docs/adr/2026-09-21-2143-...md`'s "Accepted imprecision" note). Already
  present for the plain-declaration form since #709; this plan reuses
  `lower_pattern_binding` unchanged, so it neither introduces nor fixes this.
- **Array patterns in any of the three sites** (`catch ([a = await 1])`,
  `for (var [a = await 1] of xs)`, `for (var [a = await 1] = [];;)`) —
  `lower_pattern_binding` doesn't support array patterns (#725: the iterator
  record must be held across suspension and closed exactly once on abrupt
  exit). `pattern_needs_await_lowering`'s `pattern_lowering_supported` gate
  (§3) keeps these off the compiled path, matching today's behavior exactly —
  unchanged by this plan.
- **An object rest beside a suspending sibling** (`catch ({a = await 1,
  ...rest})`) — same #725 gap `pattern_lowering_supported` already returns
  `false` for.
- **The symmetric `yield`-in-C-style-for-init gap.** `contains_suspension`'s
  `Statement::For` arm gains an *await-only* check (§3); a generator whose
  only suspension is `yield` in a `for (var {a = yield 1} = …;;)` initializer
  stays undetected, exactly as before this plan. Both ADRs record this as
  already open for both triggers; #726 is scoped to `await` only, so this
  plan does not risk an untested behavior change to sync/async-generator
  `yield` handling here (see §6's bookkeeping-mismatch risk). Track
  separately if wanted.
- **A C-style for-init pattern containing *both* `await` and `yield`** (e.g.
  an async generator's `for (var {a = await 1, b = yield 2} = {};;)`) passes
  the `await`-only `pattern_needs_await_lowering` gate (since it does contain
  an `await`) and, once routed through `transform_variable_declaration`,
  lowers the `yield` half too — reintroducing the exact yield-point-bookkeeping
  mismatch §6 flags as the reason this gate is `await`-only in the first
  place, just for a narrower input (a pattern that mixes both keywords in the
  same for-init, not a pure-`yield` one). Not tested or fixed by this plan;
  flagged here so a future fix doesn't have to rediscover it.
- **`using`/`await using` for-of heads** — grammar restricts these to a
  single `BindingIdentifier`; nothing for `pattern_contains_await` to ever
  match.
- **No refactor of `bind_pattern`'s residual `_ => JsValue::UNDEFINED`
  swallow arms** — out of scope, unrelated to this issue's binding sites.
