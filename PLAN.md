# Plan: issue #625 — is the legacy `self.generator_context`/`InlineYield` fallback still reachable?

## 1. Problem restated

`generator_transform.rs` compiles a generator/async-generator body into a `GeneratorStateMachine`
so that `.next()`/`.return()`/`.throw()` resume by jumping straight to a stored state id
(`StateTerminator::Yield`), never replaying already-executed code. A second, older mechanism
still exists alongside it: if `exec_state_machine_body` ever returns a bare `Completion::Yield`
that the transform didn't decompose into a `StateTerminator`, `generator_next_state_machine_impl`
falls back to `SentValueBindingKind::InlineYield` — it re-executes the *entire current state's
statement list* from the top on the next resume, using `self.generator_context`
(`target_yield`/`current_yield`/`GeneratorResumeKind`) to fast-forward silently past yields that
already fired. Issue #625 asks whether any real JS program still reaches that fallback, and if
so whether it should be fixed, documented, or deleted. This investigation finds a concrete,
spec-legal, currently-reachable trigger — a `yield`/`yield*` inside a class declaration's
`extends` clause or a computed method/field name — and traces it to a detection gap in the
generator-body analysis (not a gap in the state-machine transform's control-flow coverage, which
is what the issue's enumerated "zero hits" list already ruled out).

## 2. Spec basis

- **§15.7 Class Definitions, grammar** (`sec-class-definitions`): `ClassHeritage[Yield, Await] :
  extends LeftHandSideExpression[?Yield, ?Await]` and `ClassElementName[Yield, Await] :
  PropertyName[?Yield, ?Await]` both inherit the `[Yield, Await]` parameters of the enclosing
  `ClassTail`. A class declared directly inside a generator's body is parsed with `[+Yield]`
  threaded all the way down, so `extends (yield x)` and `[yield x]() {}` / `static [yield x] = 1`
  are syntactically legal generator-yield sites. `ClassStaticBlockStatementList[~Yield, +Await,
  ~Return]` is fixed regardless of context (confirmed: `sec-class-definitions-static-semantics-early-errors`
  bans `await`, and a bare `yield` there can only be a strict-mode-reserved identifier, never a
  `YieldExpression`, since the production isn't reachable with `~Yield`) — static blocks are not a
  trigger.
- **§15.7.14 Runtime Semantics: ClassDefinitionEvaluation** (`sec-runtime-semantics-classdefinitionevaluation`):
  evaluates `ClassHeritage` (and throws `TypeError` if the result `IsConstructor` is false) once,
  strictly before the loop that runs `ClassElementEvaluation` (which evaluates each element's
  `PropertyKey`/`ClassElementName`) exactly once per element, in source order. Each of these
  evaluations is ordinary synchronous `Evaluation`, running in the same execution context as the
  enclosing generator — i.e. a `yield`/`yield*` there is a real generator suspension point, not a
  nested-function boundary.
- **§15.7.11 Runtime Semantics: ClassFieldDefinitionEvaluation** (`sec-runtime-semantics-classfielddefinitionevaluation`):
  a field's `Initializer` is wrapped via `OrdinaryFunctionCreate(..., ~non-lexical-this~, ...)`
  and invoked later via `Call` during `InitializeInstanceElements`/`DefineField` — an ordinary
  (non-generator) function, and never actually reachable with `yield` in practice: **empirically
  confirmed both jsse and Node reject `x = (yield 1);` as a field initializer with a parse-time
  SyntaxError** (see Test surface below), so field initializers are excluded from this fix's scope
  on primary evidence, not just a reading of the parameterized grammar.
- **§27.5.3 Generator Abstract Operations, GeneratorResume** (`sec-generatorresume`) and
  **§27.5.1 Generator Function Definitions, Runtime Semantics: Evaluation** (`sec-generator-function-definitions-runtime-semantics-evaluation`,
  the `YieldExpression`/`YieldExpression : yield * AssignmentExpression` algorithms): a generator
  resumes exactly once past the point it suspended; re-running already-completed evaluation steps
  (and, for `yield*`, re-fetching the delegate's iterator and re-invoking already-consumed
  `next()` calls) is observably different from a spec-conforming resume whenever the replayed
  code has any side effect beyond producing the same yielded value — exactly what the InlineYield
  fallback does when it's exercised.

No change to JS syntax or semantics is being *invented* here — the fix makes jsse's generator-body
analysis recognize spec-legal yield sites it currently misses; the target behavior is what the
spec (and, for the two cases checked, Node) already mandates.

## 3. Evidence (reproduced against `target/debug/jsse`, built from this branch, and Node v26.5.0)

### 3a. Computed method key (`yield`, non-delegating)

```js
function* g() {
  console.log("before-class");
  class C {
    [yield "computed-key"]() { return 1; }
  }
  console.log("after-class", Object.getOwnPropertyNames(C.prototype));
}
const it = g();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next("sentValue")));
```

jsse output:
```
before-class
{"value":"computed-key","done":false}
before-class
after-class [object Object]
{"done":true}
```
`"before-class"` prints twice — the whole function body re-executes on resume.

### 3b. `yield*` in a static computed key, delegating to a stateful iterator

```js
function makeIterable(vals) {
  return { [Symbol.iterator]() {
    let i = 0;
    return { next() {
      console.log("inner-next-call", i);
      if (i < vals.length) return { value: vals[i++], done: false };
      return { value: undefined, done: true };
    } };
  } };
}
function* g() {
  console.log("before-class");
  class C extends null {
    static [yield* makeIterable(["a", "b"])] = 1;
  }
  console.log("after-class");
}
const it = g();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next("last")));
```

jsse output — `inner-next-call` restarts from 0 on every resume (a fresh iterator refetched
every time, extra `next()` calls burned on a real, statefully-side-effecting iterator):
```
before-class
inner-next-call 0
{"value":"a","done":false}
before-class
inner-next-call 0
inner-next-call 1
{"value":"b","done":false}
before-class
inner-next-call 0
inner-next-call 1
inner-next-call 2
after-class
{"done":true}
```

Node output (authoritative reference, matches spec-mandated single-evaluation/no-replay resume):
```
before-class
inner-next-call 0
{"value":"a","done":false}
inner-next-call 1
{"value":"b","done":false}
inner-next-call 2
after-class
{"done":true}
```

### 3c. Root cause: a detection gap, not a control-flow gap

`generator_analysis.rs` has three independent AST walkers, each with the same blind spot:
`analyze_statement`/`analyze_expression` (populates `GeneratorAnalysis.yield_points`, used by
`transform_generator_inner_opts`'s `analysis.yield_points.is_empty()` fast-path check),
`contains_yield`/`expr_contains_yield`, and `contains_suspension`/`expr_contains_suspension`
(both feed `stmt_has_suspension`/`expr_has_suspension` in `generator_transform.rs`, which gate
whether `transform_yielding_statement`/`transform_yielding_expression` decompose a construct at
all). All three explicitly stop at `Statement::ClassDeclaration`/`Expression::Class`
("Don't recurse into function/class boundaries" — correct for function bodies, **wrong** for a
class's own `extends` clause and computed keys, which run in the enclosing scope per §15.7.14).

Because `analyze_generator_body` reports zero yield points for example 3a's body, `transform_generator_inner_opts`
takes the `create_simple_machine` fast path (`generator_transform.rs:375`) — a single-state
machine holding the *entire, untransformed function body*. When the embedded `Expression::Yield`
still fires at runtime (`self.generator_context` is `None` there, so `eval.rs`'s `Yield` arm
falls straight through to `Completion::Yield`), `generator_next_state_machine_impl`'s generic
`Completion::Yield` handler (the one documented at `generator_runtime.rs:4260-4268`) has no choice
but to treat the whole body as one InlineYield-replayable unit. This is the exact mechanism the
issue asks about, and it is reachable today — not via the loop/switch/try/for-of constructs the
issue already ruled out, but via class heritage/computed keys.

**Ruled out as unrelated (do not conflate):** the issue's "unrelated anomaly" (`yield*` in a
`while` loop calling the delegate's `next()` N→N+2 times across a loop-iteration boundary) does
**not** reproduce on this branch's debug binary — a plain `while` loop wrapping `yield*` with no
class involved calls `next()` identically to Node (verified: `inner-next-call` sequence and
`.next()` results are byte-for-byte identical to Node across 6 external `.next()` calls spanning
two loop iterations). That anomaly, if real, has a different, still-unminimized cause and is
**not** addressed by this plan; it stays exactly as open as the issue left it.

**Checked and excluded:** `x = (yield 1);` as a class field initializer. Both jsse and Node throw
a parse-time SyntaxError (`Unexpected token: Keyword(Yield)` / `Unexpected strict mode reserved
word`) — despite the grammar parameter chain nominally threading `[+Yield]` into `Initializer`,
neither engine treats a field initializer as a real yield site. Not a trigger; not touched by this
fix; no interpreter-global-`generator_context`-leak concern to chase here since the construct
never parses.

## 4. Files to touch

Engine only, no tooling/CI/benchmark changes:

- `src/interpreter/generator_analysis.rs`
  - `analyze_statement` / `analyze_expression`: recurse into a class declaration's/expression's
    `super_class` and each `ClassElement`'s computed `PropertyKey`, registering `YieldPoint`s
    exactly as `Expression::Object`'s computed-key handling already does.
  - `contains_yield` / `expr_contains_yield`: same recursion (sync detection).
  - `contains_suspension` / `expr_contains_suspension`: same recursion (async detection, also
    covers `await` in an async generator's class heritage/computed keys).
- `src/interpreter/generator_transform.rs`
  - `transform_yielding_statement`: add a `Statement::ClassDeclaration` arm.
  - `transform_yielding_expression`: add an `Expression::Class` arm.
  - Both arms: if `super_class` has suspension, hoist it into a temp var via the existing
    `transform_yielding_expression(..., Some(SentValueBindingKind::Variable(tv)))` pattern *first*
    (source order — matches `ClassDefinitionEvaluation`'s heritage-before-elements order); then,
    for each `ClassElement::Method`/`ClassElement::Property`/`ClassElement::AutoAccessor` whose
    `key` is `PropertyKey::Computed(_)` and has suspension, hoist that key expression into its own
    temp var, in declaration order. Leave `ClassElement::StaticBlock` bodies, method/function
    bodies (`ClassMethod.value`), and field initializer values (`ClassProperty.value`) untouched —
    none of them are generator-yield sites reachable from the outer generator (see §2/§3c).
    Reconstruct the `ClassDecl`/`ClassExpr` with `super_class`/keys swapped for
    `Expression::Identifier(tv)` where hoisted, and emit it via `ctx.emit_statement(...)` /
    `emit_expression_with_binding(...)` exactly like the existing `Expression::Object` handling.
- `CLAUDE.md` (Architecture Notes): the current sentence — "Generators use a replay-based
  approach (re-execute the function body, fast-forwarding past previous yields)" — describes the
  now-narrow InlineYield fallback, not the dominant `StateTerminator`/state-machine path. Reword
  to state the state machine is primary and InlineYield is a fallback for constructs the
  transform doesn't yet decompose (answers the issue's open question 2).
- `test262-extra/` (new files, see §5).

No `spec/` or `test262/` edits. No new dependency.

## 5. TDD slices

1. **Detection: `analyze_generator_body` sees the yield.**
   Red: in `src/interpreter/generator_analysis.rs`'s existing `#[cfg(test)]` module, add a test
   building a generator body `[Statement::ClassDeclaration(ClassDecl { super_class: None, body:
   vec![ClassElement::Method(ClassMethod { key: PropertyKey::Computed(Box::new(make_yield())),
   .. })], .. })]` and assert `analyze_generator_body(&body, &[]).yield_points.len() == 1` and
   `contains_yield(&body[0])`. Currently fails (`yield_points` empty, `contains_yield` false).
   Green: extend `analyze_statement`/`analyze_expression`/`contains_yield`/`expr_contains_yield`
   as described in §4 for computed keys (start with the sync/`contains_yield` path; this slice
   does not yet need the async `contains_suspension` mirror).
   This slice alone does **not** fix any observable JS behavior yet — `transform_yielding_statement`
   still falls through to its `_ => ctx.emit_statement(stmt.clone())` wildcard with no state split,
   so example 3a would still replay a single (now merely smaller, no longer whole-body) state.
   Verify that explicitly with a second assertion in the same red/green pass:
   `transform_generator(&body, &[]).num_yields == 1` (was `0`, taking the `create_simple_machine`
   path).

2. **Async mirror of slice 1.**
   Red: same shape, `contains_suspension`/`expr_contains_suspension` with an `Expression::Await`
   in a computed key instead of `Expression::Yield`, for an async generator. Green: extend
   `contains_suspension`/`expr_contains_suspension` identically.

3. **Decomposition: no more replay.**
   Red: a `#[cfg(test)]` test in `generator_transform.rs` asserting `transform_generator(&body,
   &[]).states.len() > 1` for the slice-1 body (today: `1`, from `create_simple_machine`). Plus an
   end-to-end behavioral test (new `test262-extra/` file, §5 below) reproducing example 3a/3b and
   asserting the exact Node-matching output (no duplicated `console.log`, monotonic
   `inner-next-call` sequence). Green: add the `Statement::ClassDeclaration`/`Expression::Class`
   arms in `transform_yielding_statement`/`transform_yielding_expression` per §4.
   Acceptance criteria for this slice, to hold the implementer to the ordering constraint found
   during planning (§2, §3):
   - heritage is hoisted (if it needs hoisting) strictly before any element key;
   - each element key is hoisted in declaration order;
   - a class with **no** suspending heritage/keys is byte-for-byte unaffected (same states, same
     `num_yields`) — guard this with a regression test on an existing passing generator/class
     combination (e.g. adapt `cpn-class-decl-computed-property-name-from-yield-expression.js`'s
     shape without triggering suspension, to prove the common case has zero transform diff).
   - **Known, accepted limitation** (document, don't silently paper over): when a class has *both*
     a heritage expression that would throw (`extends <non-constructor>`) *and* a computed key
     that needs hoisting, the hoisted key's side effects can now run before the heritage
     `TypeError` — the "evaluate heritage and check `IsConstructor` before any element" order from
     `ClassDefinitionEvaluation` step order is not preserved when both halves are decomposed
     independently. Add a `test262-extra` regression test that pins down whatever jsse's actual
     post-fix behavior is for this combination (so a future change to it is deliberate, not
     accidental), and note in the PR description that a full fix requires making
     `ClassDefinitionEvaluation` itself resumable — out of scope here (§7).

4. **Docs.**
   Update the `CLAUDE.md` Architecture Notes sentence per §4. No test — this is prose, checked by
   review, not `cargo test`.

## 6. Test surface

- **Targeted test262 run** (should stay green, no special-casing needed — they already exercise
  this code path but don't assert on side-effect counts, which is exactly why they didn't catch
  this bug):
  `uv run python scripts/run-test262.py test262/test/language/statements/class/ test262/test/language/expressions/class/`
  (covers `cpn-class-decl-computed-property-name-from-yield-expression.js`,
  `cpn-class-decl-fields-computed-property-name-from-yield-expression.js`, and the broader
  class/generator-method corpus). Also run the generator/async-generator directories since the
  analysis functions are shared, hot-path code:
  `uv run python scripts/run-test262.py test262/test/language/statements/generators/ test262/test/language/expressions/generators/ test262/test/built-ins/GeneratorFunction/ test262/test/language/statements/async-generator/ test262/test/built-ins/AsyncGeneratorFunction/`
- **New `test262-extra/` files** (test262 doesn't assert observable side-effect counts, so the
  actual regression coverage for this bug must live here, following the existing file/header
  conventions in that directory, e.g. `async-generator-nested-activation-does-not-steal-for-await-iterators.js`):
  - `generator-class-computed-key-yield-single-evaluation.js` — example 3a shape: assert (via a
    counter, not `console.log`) that a side-effecting statement preceding the class declaration,
    and the computed-key expression itself, each run exactly once across two `.next()` calls.
  - `generator-class-heritage-yield-star-single-iterator-advance.js` — example 3b shape: assert
    the delegate iterable's `next()` call count matches Node's (monotonic, one call per external
    `.next()` at the boundary, no restart), using a counting iterator instead of `console.log`.
  - `generator-class-heritage-throw-vs-computed-key-side-effect-order.js` — the known-limitation
    case from slice 3, pinned to whatever the fixed engine actually does (documented as a known
    deviation in a comment, not asserted as "this is spec-correct").
  - `async-generator-class-computed-key-await-single-evaluation.js` — async mirror of the first
    file, using `await` instead of `yield` in the computed key.
- **`cargo test --release`** for the Rust-level unit tests added in slices 1–3
  (`generator_analysis.rs`, `generator_transform.rs`).
- Not applicable: `scripts/run-node-shim-selftest.sh`, `scripts/run-shim-fixtures.sh`,
  `scripts/run-library-tests.sh` — this change touches only generator-body analysis for a class
  syntax edge case, not any Node-compat shim surface.

## 7. Regression risk

- **Hot path**: `contains_yield`/`contains_suspension`/`analyze_statement`/`analyze_expression`
  and `transform_yielding_statement`/`transform_yielding_expression` run once per generator/async-
  generator function definition (not per call), so the risk is compile-time-transform
  correctness, not runtime hot-loop performance. The new recursion only activates for
  `Statement::ClassDeclaration`/`Expression::Class`; every other generator body is byte-for-byte
  unaffected (same code paths, same states) — plain classes with no heritage/computed-key
  suspension are the overwhelming common case and see zero transform difference (guarded by the
  slice-3 regression test).
- **`test262-pass.txt` baseline**: could move (upward, hopefully) for generator/class-adjacent
  scenarios; per project convention this PR does not roll the baseline forward (`--update-baseline`
  is a `main`-branch operation). Run the targeted directories in §6 and the full suite before
  opening the PR to confirm no regressions; report the diff in the PR description rather than
  committing a new `test262-pass.txt`.
- **Shared machinery leaned on**: the tree-walker's `exec_state_machine_body`/`eval_expr`
  (`Expression::Yield` arm, `Expression::Class` evaluation in `eval/literals.rs`), and indirectly
  `self.generator_context`/`GeneratorResumeKind`/`SentValueBindingKind::InlineYield` — this PR does
  **not** delete or restructure any of that runtime machinery, only reduces how often it's
  exercised. Per the issue's question 1: it is *not* proven dead code (this investigation audited
  one AST-node family, not an exhaustive one) and should not be deleted — leaving it in place
  keeps degraded-but-correct behavior as a backstop for any future/other undiscovered analysis gap
  instead of turning an unknown gap into a hard crash.
- **GC/ObjectKind/bytecode**: untouched — this fix is confined to the tree-walker's generator
  transform (compile-time AST rewrite), never touches `ObjectKind`, `gc.rs`, `property.rs`, or the
  `bytecode/` fast path (generators are not eligible for the bytecode VM).
- **Field initializers**: explicitly excluded per §3c evidence (parse-time SyntaxError in both
  engines) — no runtime change needed or planned there.

## 8. Out of scope

- Fully fixing the heritage-check-vs-computed-key evaluation-order edge case identified in slice 3
  (would require making `ClassDefinitionEvaluation` itself resumable/interleaved with the state
  machine, not just hoisting sub-expressions) — documented as a known limitation with a pinning
  regression test instead.
- The issue's "unrelated anomaly" (`yield*` in a `while` loop, N→N+2 `next()` calls across a loop
  boundary) — does not reproduce with this branch's binary on a minimal repro; left exactly as
  open/unminimized as the issue described. Do not attempt to explain or fix it under #625; if it
  recurs, it needs its own reduced repro and its own issue.
- Deleting `self.generator_context`/`current_yield`/`target_yield`/`GeneratorResumeKind`/
  `SentValueBindingKind::InlineYield` or the `Expression::Yield` arm's manual replay state — not
  proven dead (see §7), and deleting defensive fallback machinery is a separate, larger decision
  the issue's evidence doesn't support making yet.
- Any decomposition of `ClassStaticBlock` bodies or field `Initializer` values — provably excluded
  from being generator-yield sites (see §2/§3c), not just deprioritized.
- Refactoring the broader `generator_analysis.rs`/`generator_transform.rs` walkers beyond the
  class-specific arms (e.g. unifying the three separate "don't recurse into functions/classes"
  checks into one shared helper) — worth doing later, not bundled into this bug fix.
- Rewriting `CLAUDE.md`'s Architecture Notes beyond the one sentence identified in §4.
