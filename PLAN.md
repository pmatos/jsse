# Plan: issue #722 — `let`/`const` destructuring of a suspended initializer throws a TDZ ReferenceError

## 1. Problem restated

When a `let`/`const` declaration destructures the result of an expression that
suspends the enclosing generator or async generator (`yield`, `yield*`, or
`await`, e.g. `const { x } = yield* g()`), the generator-transform's
state-machine lowering binds the resumed value with `BindingKind::Var`
instead of the declaration's real kind. `Environment::set` (the `Var`-kind
write path) rejects an uninitialized `let`/`const` binding still in its TDZ,
so the destructuring throws `ReferenceError: Cannot access 'x' before
initialization` instead of initializing `x`. The issue reports this for
`async function*` + `yield*`; investigation below (§6) shows the same
mechanism also breaks plain `yield` in async generators and both `yield`/
`yield*` in **sync** generators — it is not `yield*`-specific or
async-generator-specific, so the fix targets the shared mechanism rather than
one call site.

## 2. Spec basis

- **`sec-runtime-semantics-bindinginitialization`** (`BindingPattern :
  ObjectBindingPattern` / `ArrayBindingPattern`, spec.html:9617) and its
  **`sec-initializeboundname`** abstract operation (spec.html:9678): when
  `environment` is not `undefined`, `InitializeBoundName` performs
  `environment.InitializeBinding(name, value)`; when `environment` is
  `undefined`, it does `ResolveBinding` + `PutValue` instead. Which one
  applies is selected entirely by which `environment` argument the caller
  passes.
- **`LexicalBinding : BindingPattern Initializer`**
  (`sec-let-and-const-declarations-runtime-semantics-evaluation`,
  spec.html:21534-21540): for `let`/`const`, `BindingInitialization` is
  called with `environment` = the running execution context's
  `LexicalEnvironment` (not `undefined`) — i.e. `InitializeReferencedBinding`
  semantics, never `PutValue`.
- **`VariableDeclaration : BindingPattern Initializer`**
  (spec.html:21594-21599): for `var`, the same `BindingInitialization` SDO is
  called with `environment` = `*undefined*` — i.e. `PutValue` semantics. This
  is the contrast case the fix must not disturb: `var` pattern destructuring
  must keep assigning into the (already-declared, already-initialized) var
  binding, not re-declare/re-initialize it.
- **`YieldExpression : yield AssignmentExpression`** and **`YieldExpression :
  yield * AssignmentExpression`**
  (`sec-generator-function-definitions-runtime-semantics-evaluation`,
  spec.html:24273, 24279): both return a value (`Yield(value)`, or
  `IteratorValue(innerResult)` once the delegated iterator reports `done`)
  that flows into the enclosing declaration's `Initializer` evaluation
  exactly like any other expression value — there is nothing `yield`/
  `yield*`-specific about how that value must then be bound; the binding step
  is governed entirely by the two clauses above.

`AwaitExpression` (`sec-await`) is the analogous case for async functions;
the same shared engine mechanism handles its resumed value, so the fix
naturally covers it too, though no observed failure exists for it today (see
§6).

## 3. Files to touch

- `src/interpreter/generator_transform.rs`
  - `transform_variable_declaration`: the fix (see §4).
  - `SentValueBindingKind` enum definition and its `Pattern(Pattern)` variant.
  - `clear_sent_value_binding` (Pattern match arm).
  - `emit_expression_with_binding` (Pattern match arm, currently hardcodes
    `VarKind::Let`).
- `src/interpreter/eval.rs` — `async_function_resume`'s `pending_binding`
  match (Pattern arm).
- `src/interpreter/eval/generator_runtime.rs` — every runtime consumer of
  `SentValueBindingKind::Pattern`: `generator_next_state_machine_impl` (two
  sites: the top-of-function `delegated_iterator`/`pending_binding` resume,
  and the inline yield*-done branch inside the dispatch loop), the
  `.throw()` counterpart of the inline yield*-done branch, `bind_yield_star_result`,
  the async-generator `pending_binding` resume, and `apply_sent_value_binding`.
- `test262-extra/` — new regression tests (see §5).
- No `docs/adr/` entry: this is a bug fix restoring documented spec behavior,
  not an architectural decision.

## 4. TDD slices

1. **Red — async generator `yield*` + `const` pattern (the issue's own
   repro).** Add
   `test262-extra/async-generator-yield-star-let-const-pattern-binding.js`
   asserting `const { x } = yield* (async function* () { return { x: 1 };
   })();` resolves `x` to `1` instead of throwing. Confirm it fails on
   `main` with today's `ReferenceError: Cannot access 'x' before
   initialization`.
2. **Red — the rest of the failing matrix**, added in the same file or
   siblings following the existing `async-generator-*.js` /
   `generator-*.js` naming convention: sync generator `yield*` pattern, async
   generator plain `yield` pattern, sync generator plain `yield` pattern (all
   four confirmed failing in §6). Keep each as an independent assertion so a
   partial fix shows partial green.
3. **Green — the fix.** In `transform_variable_declaration`
   (`generator_transform.rs`), the branch that currently builds
   `SentValueBindingKind::Pattern(pat.clone())` for a non-identifier pattern
   whose initializer suspends (and does *not* need the existing
   `pattern_needs_lowering` decomposition) instead:
   - binds the initializer to a fresh `dstr_src`-style temp via
     `SentValueBindingKind::Variable(source)` (the same shape the
     `pattern_needs_lowering` branch already uses just above it, and the
     shape every runtime consumer already binds correctly, since `Variable`
     already special-cases "still in TDZ → `initialize_binding`, else →
     `set`");
   - once `transform_yielding_expression` returns (i.e. after the resume
     state is current), emits one ordinary synchronous
     `Statement::Variable { kind: decl.kind, pattern, init: source }` via the
     existing `emit_pattern_binding(kind, pattern, source, ctx)` helper
     (already used as the leaf case inside `lower_pattern_binding`) —
     letting the interpreter's normal, already-correct `bind_pattern` apply
     `decl.kind` (`Var`/`Let`/`Const`) verbatim, exactly as it does for any
     non-suspending destructuring declaration.

   This mirrors the `pattern_needs_lowering` branch's existing "temp var +
   synchronous pattern bind" shape (already verified correct for `let`/
   `const` inside an async generator, §6 "lowered" case) rather than
   inventing a new mechanism, and it works for every pattern shape
   (`pattern_needs_lowering`'s `pattern_lowering_supported` restriction
   against `Array`/`Rest`/`MemberExpression` does not apply here, since
   `emit_pattern_binding` is unconditional).

   After this change, `SentValueBindingKind::Pattern` is constructed nowhere
   in the crate (confirmed by exhaustive grep before this plan: the
   construction this slice replaces was the *only* construction site). All
   slice-1/2 tests should pass.
4. **Refactor — delete the now-dead variant**, in the same commit as slice 3
   (the repo's `-D warnings` clippy pre-commit hook will flag
   `SentValueBindingKind::Pattern` as an unconstructed/dead enum variant the
   moment its one producer is gone, so this cannot be deferred to a later
   commit without a red build in between):
   - Remove the `Pattern(Pattern)` arm from `SentValueBindingKind`.
   - Delete the now-unreachable match arms in `clear_sent_value_binding`,
     `emit_expression_with_binding` (this also removes the pre-existing,
     separate `VarKind::Let`-hardcoding on that arm — dead code, not a
     behavior fix, see §7), `async_function_resume`, and every
     `generator_runtime.rs` site listed in §3.
   - `cargo build` must be clean (no `match` non-exhaustiveness, no dead-code
     warnings) before this slice is considered done.
5. **Regression tests**, added alongside slice 1/2's files:
   - **const-ness survives**: `const { x } = yield* ...; x = 5;` must throw
     `TypeError` ("Assignment to constant variable"), not silently succeed
     and not throw a stray `ReferenceError` — this is what would happen if
     the fix force-initialized every pattern binding as plain `Var` instead
     of routing through the real `decl.kind`.
   - **genuine TDZ still throws**: a real self-reference in the pattern
     (e.g. `const { x = x } = yield;`, resumed with a value that leaves the
     default triggered) must still throw `ReferenceError` — the fix must not
     blanket-suppress TDZ checking, only route the *correct* kind through
     it.
   - **`var` pattern regression guard**: `var { x } = yield ...;` inside an
     `if` block, read after the block, must still see the assigned value
     (function-scoped, PutValue-style per `VariableDeclaration : BindingPattern
     Initializer`) — guards against the fix accidentally changing `var`'s
     scoping by routing it through the same temp-var mechanism incorrectly.
   - **array pattern** (previously not exercised by this mechanism at all
     since `SentValueBindingKind::Pattern` covered it only in the broken
     form): `const [a] = yield* (function* () { return [1]; })();` inside a
     sync generator.
6. **Full regression pass**: targeted test262 dirs (§5) plus
   `cargo test --release` and `uv run python scripts/run-custom-tests.py`.

## 5. Test surface

Targeted test262 dirs (no existing test262 coverage was found for this exact
scenario — `grep`-style search across
`test262/test/language/{expressions,statements}/{yield,generators,async-generator}/`
turned up delegation-protocol and suspension-ordering tests, none combining
`yield`/`yield*` with a `let`/`const` destructuring target):
- `test262/test/language/statements/generators/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/expressions/generators/`
- `test262/test/language/expressions/async-generator/`
- `test262/test/language/expressions/yield/`
- `test262/test/language/statements/let/`, `test262/test/language/statements/const/`
  (destructuring-binding coverage, to catch any regression in the
  non-suspending path the fix's `emit_pattern_binding` call now also carries
  the async/generator suspending path through)
- `test262/test/language/statements/async-function/` (regression-only; §6
  found no failure here, but the shared mechanism changes)

Run with `--release` builds per project convention.

New coverage belongs in `test262-extra/` (spec-correct behavior with no
matching test262 file), following the existing `async-generator-*.js` /
`generator-*.js` naming and test262-header conventions (see e.g.
`test262-extra/async-generator-destructuring-default-await.js`):
- `async-generator-yield-star-let-const-pattern-binding.js` — the issue's
  own repro plus the `let` variant and the object/array shapes.
- `generator-yield-star-let-const-pattern-binding.js` — sync-generator
  sibling.
- `async-generator-yield-let-const-pattern-binding.js` — plain (non-star)
  `yield` in an async generator.
- `generator-yield-let-const-pattern-binding.js` — plain `yield` in a sync
  generator.
- One file covering the three regression guards from TDD slice 5
  (const-ness TypeError, self-reference TDZ, `var` still function-scoped) —
  can live alongside the above or as its own
  `generator-yield-pattern-binding-kind-regressions.js`.

## 6. Regression risk

- **Investigation performed before this plan** (binary built from this
  branch at `target/debug/jsse`, ad hoc `-e` scripts — not part of the
  fix, evidence only):
  - Confirmed failing today: async generator `yield*` + pattern (issue
    repro, both `const` and `let`); sync generator `yield*` + pattern; async
    generator plain `yield` + pattern; sync generator plain `yield` +
    pattern. All four throw the identical
    `ReferenceError: Cannot access 'x' before initialization`.
  - Confirmed already-working today (must stay working): sync generator
    `yield*` binding a plain identifier (`const r = yield* ...`); async
    function `await` + pattern, both resolved-immediately and genuinely
    suspended (`setTimeout`-delayed) promises, both `let` and `const`; async
    generator pattern whose default itself contains `await` (the
    `pattern_needs_lowering` path, already carrying `decl.kind` correctly
    today — this is the mechanism slice 3 reuses).
- **Shared machinery this leans on**: the generator-transform state machine
  (`generator_transform.rs`) and its runtime driver
  (`eval/generator_runtime.rs`) are the hot path for every generator/async
  generator/async function in the engine — this is exactly the kind of
  central, widely-exercised code the project's regression-risk guidance
  calls out. The fix changes what `transform_variable_declaration` emits for
  one specific shape (non-identifier pattern, suspending initializer, no
  await-in-pattern) and deletes dead code elsewhere; it does not change
  `exec_statement`/`eval_expr` dispatch, the property MOP, GC rooting, or the
  bytecode fast path (bytecode does not implement generators/async
  functions per existing architecture notes).
- **`test262-pass.txt` baseline**: expected to move only upward (tests that
  currently fail this exact pattern, if any exist under
  `test262/test/language/statements/{generators,async-generator}/` beyond
  what targeted search found, would newly pass). No currently-passing test
  should be affected, since the change only replaces one incorrect binding
  path with the same mechanism already proven correct for the sibling
  `pattern_needs_lowering` case.

## 7. Out of scope

- **A second, distinct bug found during investigation**: `var { x } = cond ?
  yield 1 : { ... };` inside an `if` block, when the *non-suspending*
  ternary branch is the one that executes, reads back as `undefined` outside
  the block instead of the destructured value (Node gives the real value).
  This is a `var`-hoisting-through-ternary-lowering bug in
  `emit_expression_with_binding`/`lower_optional_chain`'s sibling code for
  ternaries, unrelated to TDZ or `SentValueBindingKind::Pattern` — do not
  fix it here. File a new issue via `gh issue comment`/`gh issue create`
  referencing this investigation instead of bundling a fix.
- **`emit_expression_with_binding`'s hardcoded `VarKind::Let`** (line ~1763)
  is mentioned in §4 slice 4 only because deleting the dead `Pattern` match
  arm removes it as a side effect of removing dead code — it is not being
  separately "fixed": with the `Pattern` variant gone, this arm cannot be
  reached by anything the fix changes, so there is nothing left to fix.
- **The `Variable`-case sites lacking the TDZ-aware "already initialized?"
  check** (`self.env_set(...).ok()` with no `needs_init` branch, at the two
  inline yield*-done sites in `generator_runtime.rs` and in
  `apply_sent_value_binding`) are not touched. No failing repro was found
  for them (the plain-identifier yield* case that would exercise them, §6,
  works today), so changing them would be unverified, unrelated scope.
- **`pattern_lowering_supported`'s exclusion of `Array`/`Rest`/
  `MemberExpression`** for the *already-correct* `pattern_needs_lowering`
  path (suspension inside the pattern itself, e.g. `{ x = await y() }`) is
  untouched — this fix's new branch does not use `lower_pattern_binding` and
  has no such restriction, but the pre-existing restriction on the other
  branch is a separate, already-tracked limitation (`test262-extra/async-generator-destructuring-default-await.js`
  exercises the supported shapes) and not part of this issue.
- No formatting-only changes, no unrelated refactors of
  `generator_transform.rs`/`generator_runtime.rs` beyond what slice 4
  mechanically requires to keep the build clean.
