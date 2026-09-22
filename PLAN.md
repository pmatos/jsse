# Plan: issue #727 — `yield` in a `var`/`let`/`const` destructuring default never yields in generators

## 1. Problem restated

In a (sync or async) generator, a `yield` that appears inside the *default* of a
`var`/`let`/`const` destructuring declarator (`var {a = yield 1} = {}`, `var [a = yield 1] = []`)
never suspends the generator: `it.next()` returns `{"done":true}` on the first call
instead of `{"value":1,"done":false}`. Two independent gaps combine to cause this:
(a) the generator-transform's suspension detectors (`contains_yield`, `contains_suspension`)
never inspect a declarator's binding *pattern* — only its `init` expression — so a
statement whose only suspension lives in a pattern default is never recognized as
suspending; and (b) `bind_pattern`, which performs the tree-walking `BindingInitialization`
for these patterns, evaluates each default via `eval_expr` and then discards any
completion that isn't `Completion::Normal`/`Completion::Throw` (three `_ => JsValue::UNDEFINED`
catch-all arms), which silently destroys the `Completion::Yield` a `yield` expression
produces before it can reach the generic "raw yield bubbled out of a state body"
rescue mechanism in the state-machine runtime.

## 2. Spec basis

- **`Runtime Semantics: BindingInitialization`** for `ObjectBindingPattern` /
  `ArrayBindingPattern` / `SingleNameBinding` (`sec-destructuring-binding-patterns` /
  `sec-runtime-semantics-bindinginitialization`) — governs `var`/`let`/`const`
  destructuring, including that a `SingleNameBinding`'s `Initializer` (the default)
  is evaluated via ordinary `Evaluation`, so it can contain any expression a
  generator body can contain, including `yield`.
- **`Runtime Semantics: KeyedBindingInitialization`**
  (`sec-runtime-semantics-keyedbindinginitialization`) — the per-property algorithm
  `bind_pattern`'s object-pattern arm implements; step order (`GetV`, then
  `IsUndefined` check, then evaluate `Initializer`) is what the existing
  `lower_pattern_binding`/`lower_conditional_default` machinery already lowers for
  `await`, per ADR `2026-09-21-2143-destructuring-pattern-lowering.md`.
- **`Runtime Semantics: IteratorBindingInitialization`**
  (`sec-runtime-semantics-iteratorbindinginitialization`) — governs `ArrayBindingPattern`;
  relevant because a `yield` in an array-pattern element default is, in general,
  observable *between* iterator `next()` steps, and the iterator record must be closed
  on abrupt completion. Full correctness here (iterator record held live across a
  suspended state) is the subject of the separate, already-filed issue #725 and is
  **not** attempted by this plan — see "Out of scope."
- **`GeneratorFunction`/`AsyncGeneratorFunction` body evaluation**
  (`sec-generator-function-definitions-runtime-semantics-evaluation`,
  `sec-asyncgeneratorbody`) — establishes that a `YieldExpression` anywhere lexically
  inside the generator body (not just in statement-level position) is a valid
  suspension point; nothing in the grammar restricts `yield` from appearing inside a
  declaration's `Initializer`. This is what makes the current "engine never yields"
  behavior a bug rather than a spec-permitted choice, and what the parser already
  reflects (the program parses and runs to a wrong-but-non-throwing result today).

No new JavaScript syntax or semantics are introduced. This is a conformance fix to
existing `BindingInitialization`/generator-suspension behavior.

## 3. Files to touch

Engine:
- `src/interpreter/generator_analysis.rs` — `contains_yield`'s and `contains_suspension`'s
  `Statement::Variable` arms; `pattern_needs_lowering`; add a `pattern_contains_yield`
  helper (mirroring the existing `pattern_contains_await`, built on the existing
  `pattern_any_expr` walker).
- `src/interpreter/exec.rs` — `bind_pattern`'s signature and its three swallowing arms
  (current line references: ~1433 `Pattern::Assign` default, ~1613 computed-key,
  ~1683 `var`-fast-path default); every call site in this file (`exec_variable_declaration`,
  recursive `bind_pattern` calls for array/rest elements, the for-in/for-of head bind
  sites, `exec_try`'s catch-param bind) must be updated to match the new return shape.
- `src/interpreter/eval.rs` — `bind_pattern` call sites at the function-parameter
  destructuring path (~5092, ~5096 — mechanical update only, parameter patterns can
  never contain `yield`: it is an early SyntaxError in generator formals), the
  `let _ = self.bind_pattern(...)` catch-binding site (~9225), and the for-of head
  bind inside async-function resume (~9511).
- `src/interpreter/eval/generator_runtime.rs` — the mirrored catch-binding
  (`let _ = self.bind_pattern(...)`, two occurrences) and for-in/for-of head bind
  (two occurrences) sites inside the sync- and async-generator resume paths.
  Mechanical signature-follow updates; no behavior change intended at these sites
  (their suspension detection is issue #726's territory, not this issue's).
- `src/interpreter/generator_transform.rs` — no logic change expected beyond what
  `pattern_needs_lowering`'s widened definition already gives it (verify
  `transform_variable_declaration`'s call to `pattern_needs_lowering` at ~2152 picks
  up the wider trigger with no other edits needed).

Docs:
- `docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md` — update "The trigger
  is `await`, not `yield`" bullet and the "What this change does not cover" list
  (remove #727 from the still-open list; note the array-pattern iterator-safety
  residual now explicitly belongs to #725).
- `docs/adr/2026-09-21-2157-inline-yield-suspension.md` — update the "Known
  boundaries" note that currently states `var/let/const {a = yield 1} = {}` and
  `catch ({a = yield 1})` swallow the yield (the declaration form no longer does;
  leave the `catch` mention as-is, or note it now behaves correctly as a side effect
  without claiming #726 fixed by this PR).

No changes to `spec/`, `test262/`, `scripts/`, `benchmarks/`, or `.github/`.

## 4. TDD slices

Each slice is red (add failing test) → green (minimal production fix) → confirm no
adjacent regression before moving on. Slices are ordered so the highest-value,
lowest-risk fix (object patterns in sync generators, via the existing lowering path)
lands first, and the riskier shared-signature change (needed for array patterns)
lands last with the most scrutiny.

1. **Sync generator, object pattern, `contains_yield` + `pattern_needs_lowering` widen.**
   - Test (`test262-extra/generator-yield-in-declaration-pattern-default.js`, new):
     `function* g(){ var {a = yield 1} = {}; return a }` — first `next()` yields `1`
     with `done:false`; `next(5)` returns `{value:5,done:true}`. Add a
     side-effect-ordering variant: `let n=0; function* g(){ var {a = yield 1} = (n++, {}); return a } ...` —
     assert `n === 1` after resuming (proves the source expression isn't re-evaluated,
     which would indicate the fix accidentally fell onto the InlineYield replay path
     instead of the lowered path). Cover `let`/`const` forms and a computed key
     (`var {[yield 'k']: a} = {k: 5}`).
   - Fix: in `generator_analysis.rs`, add `pattern_contains_yield` (`pattern_any_expr(pattern, &expr_contains_yield)`,
     next to `pattern_contains_await`); widen `contains_yield`'s `Statement::Variable`
     arm to `... || pattern_contains_yield(&d.pattern)`; widen `pattern_needs_lowering`
     from `pattern_contains_await(pattern) && ...` to `pattern_contains_suspension(pattern) && ...`.
     Leave `pattern_needs_assignment_lowering` untouched (assignment-form `dstr/*-yield-expr`
     cases already pass via a different path and must not move).
   - Run: `cargo test --release generator_analysis`, then the new test262-extra file,
     then a targeted `test262/test/language/statements/variable/dstr/`,
     `.../let/dstr/`, `.../const/dstr/` sweep (no yield cases exist there today per
     research, so this is a no-regression check, not new green cases).

2. **Async generator, object pattern, `contains_suspension` widen.**
   - Test: async-generator twin of slice 1's file
     (`test262-extra/async-generator-yield-in-declaration-pattern-default.js`), using
     `flags: [async]` and the project's async-generator-draining test convention
     (match the existing `async-generator-yield-let-const-pattern-binding.js` file's
     harness pattern).
   - Fix: in `generator_analysis.rs`, replace `contains_suspension`'s `Statement::Variable`
     arm's `pattern_needs_lowering(&d.pattern)` disjunct with `pattern_contains_suspension(&d.pattern)`
     (detection must not be gated on "is this shape lowerable" — that conflation is
     exactly the bug pattern to avoid, per slice-4's array case).
   - Run: same file plus `cargo test --release generator_analysis`.

3. **`bind_pattern` stops swallowing `Completion::Yield` (shared signature change).**
   - Tests (add before touching production code):
     - `test262-extra/generator-yield-in-array-pattern-default.js`: `function* g(){ var [a = yield 1] = []; return a }`
       — same yield/resume/return shape as slice 1's array analogue.
     - A nested case: `let [a, b = yield] = [1]` inside a generator — resumes past
       the first (present, no default) element correctly.
     - A replay-artifact probe (**diagnostic, not a strict pass requirement**):
       `let n=0; function* g(){ var [a = yield 1] = (n++, []); return a }` — run it
       and record whether `n === 1` or `n === 2` after resuming. Array patterns are
       *not* lowered (ADR 2143's "array patterns" exclusion, tracked as #725), so the
       statement is expected to ride the existing `InlineYield` fast-forward/replay
       fallback, which re-executes the state's statements from the top on resume —
       `n === 2` is an accepted, pre-existing limitation of that fallback, not a
       regression introduced here. If the result is `n === 2`, write the test to
       assert that (documenting current behavior) rather than skipping it, and note
       the residual in the ADR update (item 3 of "Files to touch").
     - `for (var i=0;i<2;i++){ var [a = yield i] = []; print(a) }` inside a generator
       — must yield `0` then `1` (exercises detection *inside* an enclosing
       non-suspending-by-itself construct; would fail if slice 1's `contains_yield`
       widening were the only change, since the loop body's own suspension detection
       also walks through `Statement::Variable`).
   - Fix: change `bind_pattern`'s return type from `Result<(), JsValue>` to
     `Completion` (`Completion::Normal(JsValue::UNDEFINED)` on success,
     `Completion::Throw` unchanged, propagate `Completion::Yield` — and any other
     non-Normal/Throw completion, none of which are actually reachable here —
     verbatim from the three swallowing `eval_expr` match arms at exec.rs's
     `Pattern::Assign` default, the computed-key arm, and the `var`-fast-path default).
     Update every call site listed in "Files to touch" to match on `Completion`
     instead of `Result`; call sites that currently use `?` become explicit
     early-return-on-non-Normal; call sites that currently do
     `if let Err(e) = ... { return Completion::Throw(e) }` become
     `match ... { Completion::Normal(_) => {}, other => return other }`; the two
     `let _ = self.bind_pattern(...)` catch-binding sites keep discarding (out of
     scope — pre-existing behavior per #739's finding that catch-binding already
     drops even `Throw` there; not to be silently "fixed" as a drive-by in this PR).
   - Run: full workspace `cargo test --release` (this touches ~16 call sites across
     3 files, including both generator-runtime resume paths — the highest-regression-risk
     slice), then the new tests, then the full test262 suite (step 6 below).

4. **Full-suite confirmation.** No new production code; run the complete test262
   suite and the custom test262-extra/tests suite once, to catch any of the
   already-passing ~120-156 `dstr/*-yield-expr` (assignment-form/for-of-head) cases
   or the (initially-empty per research) `dstr` declaration-form cases moving.

## 5. Test surface

- `test262/test/language/statements/variable/dstr/`,
  `test262/test/language/statements/let/dstr/`,
  `test262/test/language/statements/const/dstr/`,
  `test262/test/language/statements/generators/dstr/`,
  `test262/test/language/statements/try/dstr/` — run targeted; per research these
  directories currently contain **zero** `yield`-named cases (the `src/dstr-binding`
  test262-source case family has no `yield-expr.case`, unlike `src/dstr-assignment`),
  so this is a no-regression sweep, not a source of new green tests. If any of
  these directories does contain a case this plan's changes affect, that's new
  information to fold into slice 3/4 before merging.
- `test262/test/language/statements/for-of/dstr/*-yield-expr.js` (39 files),
  `test262/test/language/statements/for-in/dstr/*-yield*.js` (14 files),
  `test262/test/language/expressions/assignment/dstr/*-yield*.js` (39 files),
  `test262/test/language/statements/for-await-of/*-dstr-*-yield-*.js` (28 files) —
  the ~120 already-passing assignment-pattern/for-head family this plan must not
  regress. These go through `ForInOfLeft::Pattern`/`Expression::Assign` handling and
  `pattern_needs_assignment_lowering` (untouched by this plan), not through
  `bind_pattern`'s declaration-form arms, so risk is low but must be verified by
  running these directories targeted after slice 3.
- New `test262-extra/` files (flat, kebab-case, test262 frontmatter+`assert` style,
  matching the existing `generator-yield-*-pattern-binding.js` naming convention):
  - `generator-yield-in-declaration-pattern-default.js` (slice 1: object pattern, var/let/const, computed key)
  - `async-generator-yield-in-declaration-pattern-default.js` (slice 2)
  - `generator-yield-in-array-pattern-default.js` (slice 3: array pattern, nested, for-loop-enclosed)
  - Optionally fold the `n` side-effect-ordering assertions into the above files
    rather than separate files, to match the existing convention of one file per
    scenario family covering several `assert.sameValue` checks.
- `cargo test --release` — covers `generator_analysis.rs`'s existing unit tests
  (which already assert `pattern_needs_lowering`/`pattern_contains_await`/
  `pattern_contains_suspension` behavior around lines 1652-1669; these will need
  their `pattern_needs_lowering` assertions revisited since its definition changes
  from an await-gate to a suspension-gate) and the whole workspace test suite that
  exercises `bind_pattern`'s many call sites indirectly.

## 6. Regression risk

- **`bind_pattern`'s signature change is the highest-risk element.** It has ~16 call
  sites across `exec.rs`, `eval.rs`, and `eval/generator_runtime.rs` (the latter
  duplicated across what appear to be sync- and async-generator resume paths).
  A mechanical mismatch at any site (e.g. treating `Completion::Throw` as success,
  or forgetting to propagate `Completion::Yield` at one of the two catch-binding
  `let _ = ...` sites in a way that changes behavior there) would be silent until a
  specific test262 case or test262-extra regression exercises it — hence slice 3
  runs the *full* `cargo test --release` and full test262 suite, not a targeted
  subset.
- **Widening `pattern_needs_lowering` from await-only to suspension-aware** changes
  which statements the transform routes through `lower_pattern_binding` versus the
  plain tree-walker path. This predicate is also read by `contains_suspension`
  (generator_analysis.rs:1154) — confirmed both read sites are updated together in
  slices 1-2 so they stay consistent; `pattern_needs_assignment_lowering` is a
  separate predicate and is explicitly left alone so the assignment-form family
  (test surface, second bullet) doesn't move.
- **The tree-walker hot path** (`eval_expr`/`exec_statement`) is not touched;
  `Completion::Yield`'s existing propagation contract elsewhere in the interpreter
  is unchanged — `bind_pattern` moves from being an exception to that contract to
  conforming with it.
- **GC rooting / `gc_safepoint()`**: no new object kinds or held references;
  `bind_pattern` continues to operate on `JsValue`/`EnvRef` it already had.
  No `ObjectKind` variants are added.
- **Bytecode fast path** (`bytecode/`): `bytecode_enabled` is off by default; if any
  bytecode-side binding logic independently implements pattern binding, it is out of
  scope for this plan (grep for a bytecode-side `bind_pattern` equivalent during
  implementation; if one exists and is reachable while `bytecode_enabled` is off,
  no action is needed for this PR).
- **Node-compat library harnesses**: none of the pinned libraries (`decimal.js`,
  `acorn`, `zod`, `luxon`, `moment`, etc.) are known to rely on generator
  destructuring-default suspension timing; no expected interaction, and this plan
  does not propose running the library harnesses as part of its own gate — the
  standard `cargo test --release` + full test262 run is the gate.
- **Baseline**: `test262-pass.txt` is read from `origin/main` per project convention;
  this plan does not touch or roll it forward. Any newly-passing declaration-form
  `dstr` case (none expected to exist per research, but the "no regression sweep"
  in slice 4 covers the possibility) would be new baseline territory for `main`,
  not this branch.

## 7. Out of scope

- **Full array-pattern iterator-safety correctness under suspension** (issue #725):
  holding the iterator record live across a suspended state and closing it exactly
  once on abrupt completion (`.return()`/`.throw()` while suspended inside an array
  pattern's default). Slice 3 makes array-pattern yields *suspend and resume with
  the correct value*, riding the existing `InlineYield` replay fallback, but does
  not give array patterns their own lowering into state-machine steps the way
  `lower_pattern_binding` already does for object patterns. This is a materially
  larger change (per ADR 2143: "new interpreter-internal helpers, not a
  transform-only change") and is explicitly deferred.
- **`catch ({a = yield 1})` and `for (var {a = yield 1} of x)` / `for (var {a = yield 1} in x)`**
  (issue #726): this plan's `bind_pattern` signature change mechanically flows
  through these call sites (they must still type-check), and may incidentally start
  propagating a `yield` correctly there as a side effect — slice 3's test list
  includes one smoke assertion recording whatever that behavior turns out to be,
  but this plan does **not** claim #726 fixed, does not widen `contains_yield`'s
  `ForIn`/`ForOf`/`Try` arms to look at their patterns (a separate, deliberate gap
  matching those statement kinds' own detection arms, left untouched), and does not
  add dedicated test262-extra coverage for the catch/for-head forms.
- **Object-rest beside a suspending sibling** (`{a = yield 1, ...rest}`) — covered
  by `pattern_lowering_supported`'s existing `false` result for that shape (per ADR
  2143); no change to `pattern_lowering_supported` itself is planned, only to the
  gate (`pattern_needs_lowering`) that decides whether to consult it based on
  suspension kind.
- **Refactoring `Completion`'s variant set, the `propagate!` macro, or any other
  `Result<(), JsValue>`-returning helper** to a `Completion`-returning convention.
  Only `bind_pattern` is converted, because it is the one place this issue's
  reproduction requires it; no drive-by consistency pass on sibling helpers.
- **Rewording or removing the `let _ = self.bind_pattern(...)` discard at the two
  catch-binding sites** beyond the mechanical type-follow needed for the signature
  change — the discard itself (which also silently drops `Throw` today, per #739's
  finding) is pre-existing behavior outside this issue's scope.
- **Rolling `test262-pass.txt` forward** — a `main`-branch operation, not performed
  from this branch regardless of outcome.
