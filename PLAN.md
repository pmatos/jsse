# Plan: issue #774 — lower `await` in array-pattern defaults at catch-param/for-in-of-head/for-init sites

## 1. Problem restated

`catch ([a = await 1]) {}`, `for (var [a = await 1] of it) {}`, `for (var [a,
b = await 1] in obj) {}`, and `for (var [a = await 1] = x;;)` all still
evaluate the array pattern's `await` default on the blocking tree-walker path
(`await_value`, which drains the microtask queue inline) instead of
suspending the async function/generator at a real `Await` state — the same
job-ordering bug #725 fixed for a plain `var [a = await 1] = []`, and that
#773 already fixed for an *object* pattern at these same three sites. The gate
that decides whether to desugar the real pattern out of the single
non-suspending `EnterCatch`/`ForOfHead` binding call (or, for the C-style
`for`-init site, whether to route through the full per-statement transform at
all) — `pattern_needs_await_lowering` / `pattern_lowering_supported(...,
ConstrainedDeclaration)` in `src/interpreter/generator_analysis.rs` —
explicitly excludes `Pattern::Array`, so an array pattern at these sites never
triggers the existing desugar and the raw pattern reaches the tree-walker's
blocking `await_value` instead.

Code tracing (see "Spec basis is not this issue's problem" note in Files to
touch) shows the desugar these three sites already share
(`hoist_suspending_pattern` for catch/for-in-of-head; the direct
`pattern_needs_await_lowering` check inside `transform_for_statement`'s
`ForInit::Variable` arm for C-style for-init) does **not** need to "drive the
array terminator" itself, contrary to the doc comments currently on
`PatternLoweringForm::ConstrainedDeclaration` and
`pattern_lowering_supported`. It hoists the *whole* pattern to a trivial
`Pattern::Identifier(temp)` bound by the single non-suspending call, and
re-homes the real pattern as a synthesized `let <pattern> = <temp>;`
statement emitted into the ordinary statement stream (the catch body, the
loop body, or in-place for for-init). That synthesized statement is just a
normal `Statement::Variable`, detected by `contains_suspension`'s
unconstrained `pattern_needs_lowering` check and lowered by
`transform_variable_declaration` → `lower_pattern_binding` →
`lower_array_pattern_binding` — the array-iterator state machine `#772`
already built, which has supported array patterns in the unconstrained
declaration form since it landed. Proof this is already shape-agnostic today:
a **`yield`**-only array pattern at a catch param already takes exactly this
hoist (`hoist_suspending_pattern`'s guard is
`!pattern_contains_yield(pattern) && !pattern_needs_await_lowering(pattern)`,
so a `yield` anywhere triggers the hoist regardless of `pattern_needs_await_lowering`'s
shape gate) and already drives `ArrayPatternIter` — Slice 0 below pins this
down with a test that is expected to pass before any production code changes.

The fix is therefore a one-line widening of the shape gate, not new
`StateTerminator` plumbing or `EnterCatch`/`ForOfHead` runtime changes: teach
`pattern_lowering_supported`'s `Pattern::Array` arm to also accept
`PatternLoweringForm::ConstrainedDeclaration`, which flows through to every
call site that currently special-cases array patterns as unsupported at
these three sites (`pattern_needs_await_lowering`,
`for_in_of_variable_head_contains_await`, `contains_suspension`'s `Try`/`For`
arms, `hoist_suspending_pattern`'s gate, and the direct check in
`transform_for_statement`).

Object rest beside a suspending sibling (`{a = await 1, ...rest}`) remains
excluded at these sites — that is issue #771, out of scope here (see "Out of
scope").

## 2. Spec basis

- **`sec-runtime-semantics-catchclauseevaluation`** (CatchClauseEvaluation) —
  step 4, `BindingInitialization` of the `CatchParameter` with the thrown
  value. The catch-parameter site.
- **`sec-runtime-semantics-forinofheadevaluation`** (ForIn/OfHeadEvaluation)
  and **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`**
  (ForIn/OfBodyEvaluation) — the per-iteration `BindingInitialization` of the
  loop head's pattern against the stepped value. The for-in/for-of/for-await-of
  head site.
- **`sec-for-statement`** / **`sec-forbodyevaluation`** — the C-style `for`
  statement's `VariableDeclarationList` init, evaluated once before the loop
  via ordinary `BindingInitialization`. The for-init site.
- **`sec-runtime-semantics-iteratorbindinginitialization`**
  (IteratorBindingInitialization for `ArrayBindingPattern` /
  `SingleNameBinding`) — "If `iteratorRecord.[[Done]]` is `false`... `Let v
  be` the stepped value... `If Initializer is present and v is undefined`,
  then `Let defaultValue be ? Evaluation of Initializer`." This is the step
  whose `await` must suspend the function instead of blocking — the same
  clause #772 already cited for the unconstrained-declaration array case;
  this issue extends its reach to the three constrained sites.
- **`sec-await`** (Await ( value )) — step 9's `PerformPromiseThen` /
  step 10's removal of the execution context from the stack is what the
  tree-walker's blocking `await_value` skips, producing the wrong job order.

This is a pure bug fix restoring spec-mandated evaluation order at sites the
engine already special-cases; it introduces no new syntax or semantics beyond
what `sec-runtime-semantics-iteratorbindinginitialization` already requires.

## 3. Files to touch

Engine:
- `src/interpreter/generator_analysis.rs` — widen `pattern_lowering_supported`'s
  `Pattern::Array` arm (currently `if form == PatternLoweringForm::Declaration`)
  to also accept `PatternLoweringForm::ConstrainedDeclaration`. Update the
  doc comments that currently claim a structural "no way to drive the array
  terminator" limitation on `PatternLoweringForm::ConstrainedDeclaration`,
  `pattern_lowering_supported`, `pattern_needs_await_lowering`, and
  `for_in_of_variable_head_contains_await` — they predate the realization
  that the existing hoist-to-temp desugar already routes through the
  unconstrained lowering path and so needed no per-site terminator-driving
  capability. Flip the two unit tests the widening makes stale (see Slice 1).
- `src/interpreter/generator_transform.rs` — no functional change expected
  (the hoist/dispatch call sites already consult the now-widened predicate);
  update the doc comments on `hoist_suspending_pattern`,
  `transform_for_in_of_loop`, and `transform_try_statement`'s catch-param
  comment block that currently describe the array exclusion as permanent, to
  note it is now lifted for `await` (an array pattern's `yield` at these
  sites is untouched — see Out of scope).
- `src/interpreter/tests.rs` and/or the `#[cfg(test)]` module at the bottom
  of `generator_transform.rs` — new unit tests (Slices 0, 2–5).

Docs (no new ADR — this closes a gap an existing decision already covers;
only its stale "not yet reached" notes need correcting):
- `docs/adr/2026-09-30-2038-array-pattern-iterator-binding-lowering.md` —
  the "Regression risk callouts" section currently says "`for-of`/`for-in`
  heads and `catch` params are untouched — both bind through
  `ForInOfLeft`/`EnterCatch` directly, never through
  `transform_variable_declaration`" and "`for (var [a = await 1] = x;;)`
  initializers stay out of scope". Both statements are stale relative to
  current `main` (post-#773 merge) even before this issue's fix — #773
  already routes the for-init site through `pattern_needs_await_lowering`
  and the catch/for-in-of-head sites through `hoist_suspending_pattern`+
  `transform_variable_declaration`, just gated off for array shapes. Correct
  both bullets to describe the current (pre-fix) gate precisely, then note
  #774 lifts it for `await`.
- `docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md` — the
  "Catch parameters, for-in/of heads, and C-style for-init patterns ... *are*
  covered, by #726 for `await`" paragraph (lines ~100-107) describes object
  patterns only; add a short note that #774 extends the same coverage to
  array patterns via the `pattern_lowering_supported` widening.
- `docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md` — the
  "Left as residual" bullet (~184-188) about array patterns at loop heads
  hitting the #725 replay-restarts-the-iterator gap: clarify this remains
  true for `yield` (untouched, out of scope) but is now closed for `await`
  by #774.
- `CONTEXT.md` — only if a new term is introduced; this issue introduces none
  (it widens an existing predicate's domain), so no edit expected. Confirm at
  implementation time that `ConstrainedDeclaration` terminology already
  present from #773 still matches usage.

No parser, lexer, or `ast.rs` changes — the AST shapes (`Pattern::Array` at
these three binding sites) already parse correctly today; only the
state-machine transform's gating changes.

## 4. TDD slices

1. **Characterization (expected green, not red) — pin down that the hoist is
   already shape-agnostic for `yield`.** Add a unit test near
   `test_plain_await_using_for_of_head_is_lowered` in `generator_transform.rs`'s
   test module: `function* g() { try { throw 1 } catch ([a = yield 1]) {} }`
   via `transform_generator`, asserting the resulting state machine contains
   an `ArrayPatternIter` terminator. This should pass unmodified on the
   current branch — it is the evidence that no new terminator-driving
   mechanism is needed for the `await` fix, and it guards against a future
   regression re-introducing the belief that `EnterCatch`/`ForOfHead` must
   drive the array iterator themselves. If this test is unexpectedly red,
   stop and re-diagnose before touching the predicate — the plan's central
   hypothesis would be wrong.
2. **Red: widen the shape gate.** In `generator_analysis.rs`'s test module,
   change `pattern_needs_await_lowering_is_shape_gated`'s
   `assert!(!pattern_needs_await_lowering(&declared_pattern("var [a = await 1] = [];")))`
   to `assert!(pattern_needs_await_lowering(...))`, and flip/rename
   `contains_suspension_ignores_unsupported_await_catch_param_shape` (it
   currently asserts `!contains_suspension(&first_statement("try {} catch
   ([a = await 1]) {}"))`) to assert `contains_suspension(...)` is now `true`
   — rename it to something like
   `contains_suspension_detects_await_catch_array_param`. Both go red.
   Green: change `pattern_lowering_supported`'s `Pattern::Array` arm guard
   from `form == PatternLoweringForm::Declaration` to
   `matches!(form, PatternLoweringForm::Declaration | PatternLoweringForm::ConstrainedDeclaration)`.
3. **Red→green: catch-param transform integration.** Unit test in
   `generator_transform.rs`: `async function f() { try { throw 1 } catch
   ([a = await 1]) {} }` via `transform_async_function`, asserting the state
   machine contains both an `EnterCatch` terminator (with a trivial
   `Pattern::Identifier` param, confirming the hoist fired) and an
   `ArrayPatternIter` terminator (confirming the re-homed statement was
   lowered). Red before slice 2's fix, green after — no further production
   change expected since `hoist_suspending_pattern` already consults the
   now-widened predicate.
4. **Red→green: for-in/for-of-head transform integration.** Two unit tests
   (or one parametrized): `for (var [a, b = await 1] of it) {}` and
   `for (var [a, b = await 1] in obj) {}` inside an `async function`, each
   asserting both a `ForOfHead` terminator and an `ArrayPatternIter`
   terminator appear. (Two elements, not one — `for-in`'s stepped value is a
   single-character property-key string; `[a = await 1]` alone destructures
   that one code unit and never reaches the default. `[a, b = await 1]`
   forces the default once the string is exhausted.)
5. **Red→green: C-style for-init transform integration.** Unit test:
   `async function f() { for (var [a = await 1] = x;;) { break; } }`,
   asserting an `ArrayPatternIter` terminator appears (this site never used
   `EnterCatch`/`ForOfHead`, so no paired terminator to check — just confirm
   `transform_variable_declaration` was reached instead of the pattern being
   emitted as an intact, unlowered `Statement::Variable`).
6. **Refactor.** Once slices 2–5 are green, re-read
   `pattern_lowering_supported` end to end and confirm the `Declaration |
   ConstrainedDeclaration` match arms read cleanly (no leftover dead branch,
   no duplicated recursion) — this is a one-line widening, so refactor scope
   is limited to comment accuracy, not structure.
7. **Black-box regression tests** (test262-extra, red against the
   pre-slice-2 binary, green after): see Test surface below for the exact
   files — these are the deliverable-facing proof the fix observably changes
   job ordering, mirroring #773's per-site test262-extra files but with
   array-pattern shapes.

## 5. Test surface

**Targeted test262 directories to run** (none are expected to gain new
passes — test262 does not appear to test this microtask-ordering edge case
at these sites; this is a baseline-stability check, not the primary
evidence):
- `test262/test/language/statements/try/`
- `test262/test/language/statements/for/`
- `test262/test/language/statements/for-in/`
- `test262/test/language/statements/for-of/`
- `test262/test/language/statements/for-await-of/`
- `test262/test/language/statements/async-function/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/expressions/async-function/`
- `test262/test/language/expressions/async-generator/`
- `test262/test/language/expressions/async-arrow-function/`

Confirmed via `grep -rlE 'catch *\(\s*\[|for *\(\s*(var|let|const)\s*\[' test262/test/language | xargs grep -l await`
that no existing test262 file exercises an array pattern with an `await`
default at any of these three sites, so this change cannot itself move
`test262-pass.txt`.

**New `test262-extra/` files** (spec-correct behavior test262 does not
cover), mirroring #773's naming and structure
(`test262-extra/async-function-catch-destructuring-default-await-order.js`
etc.) with array-pattern shapes, each covering both a plain `async function`
and an `async function*` generator the way the existing catch-order file
does (`viaCatch` / `viaAsyncGenerator`):

1. `test262-extra/async-function-catch-array-destructuring-default-await-order.js`
   — `catch ([a = await 5])`, `esid: sec-runtime-semantics-catchclauseevaluation`.
2. `test262-extra/async-function-forof-array-destructuring-default-await-order.js`
   — `for (var [a, b = await 1] of it)`,
   `esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`.
3. `test262-extra/async-function-forin-array-destructuring-default-await-order.js`
   — `for (var [a, b = await 1] in obj)`, same `esid` as above.
4. `test262-extra/async-function-for-await-of-array-destructuring-default-await-order.js`
   — `for await (var [a, b = await 1] of it)`, same `esid` as above.
5. `test262-extra/async-function-for-init-array-destructuring-default-await-order.js`
   — `for (var [a = await 1] = x;;)`,
   `esid: sec-runtime-semantics-iteratorbindinginitialization` (the array
   pattern's own default-evaluation clause — #773's object-pattern
   equivalent cited `sec-runtime-semantics-keyedbindinginitialization`
   instead, since that one bound an object pattern).
6. `test262-extra/async-function-catch-array-destructuring-default-await-rejects.js`
   — the default's `await` rejects; assert the catch body never runs and the
   async function's promise rejects with that reason (mirrors
   `async-function-catch-destructuring-default-await-rejects.js`, array
   shape). Array-specific risk this covers that the object-pattern test
   can't: the `$dstr_iter` opened by `ArrayPatternIter::Init` must still be
   closed (or correctly left alone, per IteratorBindingInitialization's own
   abrupt-completion rules) when the default rejects mid-pattern.
7. `test262-extra/async-function-forof-array-destructuring-default-await-unwind.js`
   — the default's `await` is pending when the generator is `.return()`-ed;
   assert via an instrumented iterable that **both** the inner array-pattern
   iterator (opened for the head's own `[a = await 1]`) and the outer
   for-of iterable's iterator have their `return()` called exactly once
   (mirrors `async-function-forof-destructuring-default-await-unwind.js`,
   which only had one iterator to check since object patterns don't open
   one).

If slice 7's unwind or reject test goes red for a reason other than the
shape gate (e.g. an `ArrayPatternIter` abrupt-exit path that was never
reachable from inside a `TryContextInfo`/`for_of_depth` nesting before this
change), that is new work the issue's own "why deferred" note anticipated
possibly needing — stop, diagnose, and if real, split it into a follow-up
issue rather than silently expanding this PR's scope.

**Non-engine gate:** none — this is a pure `src/interpreter/` change. Run
`cargo test --release` for the unit tests (slices 0–5) and
`uv run python scripts/run-test262.py test262-extra/` plus the targeted
test262 dirs above for the black-box slices.

## 6. Regression risk

- **What could move `test262-pass.txt`:** nothing, per the grep above — no
  existing test262 test exercises this shape. Still run the full suite per
  CLAUDE.md's standing instruction to confirm zero regressions; do not
  update the baseline (that is a `main`-branch operation).
- **`contains_suspension` widening.** An async function/generator whose
  *only* suspension is an array pattern default at one of these three sites
  now takes the full state-machine path instead of the pre-existing
  simple/fully-tree-walked machine. This is the intended behavior change,
  shared with every prior PR in this series (#725, #772, #773) — same
  machinery, same risk shape, already exercised by the existing
  `test_plain_await_using_for_of_head_is_lowered`-style tests confirming the
  "simple machine vs real state machine" boundary.
- **Shared machinery leaned on:** `lower_array_pattern_binding`'s
  `ArrayPatternIter` terminator and its `Init`/`Step`/`Drain`/`Finish` ops
  (already shipped by #772, now reachable from three more call sites);
  `hoist_suspending_pattern`'s temp-hoist (already shipped by #773, now
  firing for a pattern shape it previously declined); `EnterCatch`/
  `ForOfHead`'s non-suspending single-bind call (unchanged — it only ever
  sees the trivial `Pattern::Identifier` post-hoist, exactly as for object
  patterns already). No GC rooting, `ObjectKind` match, property MOP, or
  bytecode-compiler surface is touched — the bytecode fast path has no
  exposure to generator/async-function state machines at all (noted in
  #772's own ADR).
- **Nested-shape interactions to verify, not assumed safe:** an array
  pattern nested inside an object pattern's property at these three sites
  (`catch ({ x: [a = await 1] })`) should already work today (object-pattern
  recursion into `Pattern::Array` wasn't shape-gated independently —
  `pattern_lowering_supported`'s `Pattern::Object` arm recurses with the same
  `form`, so this was already blocked pre-fix and now opens up too); confirm
  with one of the transform unit tests in slice 3 or a quick manual check,
  but do not add a dedicated test262-extra file for it unless it turns up
  broken — it is not a new code path, just a new reachable combination of
  two already-widened arms.
- **Object rest beside an array-pattern sibling stays excluded**, correctly:
  `pattern_lowering_supported`'s `ObjectPatternProperty::Rest` arm still
  requires `form == PatternLoweringForm::Declaration` regardless of what the
  top-level pattern shape is, so `catch ([{ a = await 1, ...r }])` still
  declines and falls back to the tree-walker/replay path, unchanged by this
  issue (tracked by #771).

## 7. Out of scope

- **`yield`-only array patterns at these three sites.** Already hoisted
  today (slice 0 proves it), but still subject to the pre-existing #725
  "replay restarts the iterator" gap once hoisted into a multi-element loop
  head, per `docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md`'s
  "Left as residual" note. This issue's predicate is deliberately `await`-only
  (`pattern_contains_await`, not `pattern_contains_suspension`) specifically
  so it does not touch that already-tracked, separate gap.
- **Object rest beside a suspending sibling at these three sites** (issue
  #771) — e.g. `catch ([{ a = await 1, ...r }])`'s inner object rest, or
  `catch ({ a = await 1, ...r })` directly. Unaffected by this change; the
  `ObjectPatternProperty::Rest` arm's `Declaration`-only guard is untouched.
- **Destructuring-*assignment*-form array patterns at the for-in/of-head
  site** (`for ([a = await 1] of x)`, no `var`/`let`/`const`) —
  `pattern_needs_assignment_lowering` has no array arm at all
  (`lower_pattern_assignment` only handles object patterns); genuinely
  different lowering pipeline, not touched by widening
  `PatternLoweringForm::ConstrainedDeclaration`, which only governs the
  declaration form.
- **Rewriting `PatternLoweringForm` into two variants** (e.g. collapsing
  `Declaration`/`ConstrainedDeclaration` now that `Array` behaves the same
  in both) — `ConstrainedDeclaration` still differs from `Declaration` on
  the `ObjectPatternProperty::Rest` arm, so the enum still earns its keep;
  do not refactor it away.
- **Any new ADR.** This closes a gap three existing ADRs already describe
  (and, in two cases, describe inaccurately post-merge); correcting those
  three is in scope, writing a fourth is not.
- **Formatting-only or unrelated cleanup** in any touched file.
