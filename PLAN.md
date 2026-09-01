# Plan: issue #552 — trailing comma after object rest accepted in assignment patterns

## 1. Problem restated

jsse parses `0, {...rest,} = {}` without error, but the ECMAScript grammar for
`ObjectAssignmentPattern` has no production that allows a comma after an
`AssignmentRestProperty`, so this must be a `SyntaxError` at parse time. The
same construct is already correctly rejected for the analogous *binding*
pattern (`var {...rest,} = {}`), and jsse already rejects the equivalent
*array* case (`[...rest,] = []`) in assignment patterns — only the object
*assignment*-pattern path is missing the check. The gap is not one missing
`if`: jsse has two independent reinterpretation paths that turn a parsed
`ObjectLiteral` expression into a destructuring target
(`validate_destructuring_pattern` in `src/parser/expressions.rs`, used for
plain `{...} = expr` assignment expressions, and `expr_to_pattern` in
`src/parser/mod.rs`, used for `for-in`/`for-of` heads and for arrow-function
parameter lists), and both are missing the equivalent of the check their
`Expression::Array` arm already has.

## 2. Spec basis

`spec/spec.html`, clause `sec-destructuring-assignment` ("Destructuring
Assignment", Supplemental Syntax), grammar `ObjectAssignmentPattern`
(lines 20926–20930):

```
ObjectAssignmentPattern[Yield, Await] :
  `{` `}`
  `{` AssignmentRestProperty[?Yield, ?Await] `}`
  `{` AssignmentPropertyList[?Yield, ?Await] `}`
  `{` AssignmentPropertyList[?Yield, ?Await] `,` AssignmentRestProperty[?Yield, ?Await]? `}`
```

There is no production of the shape `{ AssignmentRestProperty , }` nor
`{ AssignmentPropertyList , AssignmentRestProperty , }` — a comma is only ever
permitted *before* an optional rest property, never after one. `{...rest,}`
matches none of the four alternatives, so it is a grammar violation (reported
by us, as elsewhere in this file, as a parse-time `SyntaxError`), not merely
an early-error static-semantics rule.

The same clause is used as the cover-grammar refinement for `for-in`/`for-of`
LHS patterns (`sec-for-in-and-for-of-statements-runtime-semantics-labelledevaluation`,
step "Let assignmentPattern be the parse of the source text corresponding to
lhs using AssignmentPattern as the goal symbol" — quoted directly in the two
statement-form test262 fixtures), so the same restriction applies there.

For reference, the *binding* form (`ObjectBindingPattern`, clause
`sec-destructuring-binding-patterns`) has the symmetric grammar shape
(`{ BindingPropertyList , BindingRestProperty }`, no trailing comma after
`BindingRestProperty`), which is why `var {...rest,} = {}` is already
rejected today — that path (`parse_object_pattern` in
`src/parser/declarations.rs`) implicitly enforces it by requiring `}`
immediately after the rest binding, with no comma-skipping logic at all. It
needs no change.

`{...x,}` remains valid as a plain `ObjectLiteral` expression (not in
assignment-pattern position) — `ObjectLiteral`'s `PropertyDefinitionList`
production allows a trailing comma unconditionally, spread included. The
restriction above applies only when the cover grammar is refined to
`ObjectAssignmentPattern` / `ObjectBindingPattern`, exactly mirroring how
`[...x,]` is already handled for arrays via `Expression::Array`'s
`trailing_comma_after_spread` field.

## 3. Files to touch

Engine only, under `src/`:

- `src/ast.rs` — change `Expression::Object(Vec<Property>)` to
  `Expression::Object(Vec<Property>, bool)` (the new bool mirrors
  `Expression::Array`'s existing `trailing_comma_after_spread`), and update
  the four `Expression::Object(props)` match arms in this file (lines ~838,
  ~1056, ~1423, ~1566) to `Expression::Object(props, _)` — none of them care
  about trailing-comma shape.
- `src/parser/expressions.rs`:
  - `parse_object_literal` (~line 1569) — track whether the last property was
    a spread immediately followed by a comma then `}`, exactly like
    `parse_array_literal` already does for `trailing_comma_after_spread`
    (~line 1526), and thread it into the returned `Expression::Object`.
  - `validate_destructuring_pattern`'s `Expression::Object` arm (~line 149) —
    add the rejection: if the pattern ends in a rest property *and* the
    tracked trailing-comma flag is set, error
    `"Rest element must be last element in object destructuring pattern"`
    (reusing the message already used a few lines above for `{...rest, x}`).
    This is the fix for plain assignment expressions
    (`0, {...rest,} = {}`).
  - Other `Expression::Object(_)` sites in this file that don't care about
    the trailing-comma bit (~lines 85, 195, 203, 1466, 1556) must still be
    widened to match the new arity — `Expression::Object(_)` no longer
    compiles against a two-field tuple variant, so these become
    `Expression::Object(..)` (or `Expression::Object(_, _)`); this includes
    the `Expression::Array(..) | Expression::Object(_)` arm in
    `validate_destructuring_target` (~line 195) and the `if let
    Expression::Object(props) = expr` at ~line 1556. `1615`
    (`parse_object_literal`'s own `Ok(...)`) gets the new second field.
  - `parse_arrow_head`'s (or equivalent) second arrow-parameter conversion
    site, ~line 2272 (`.map(expr_to_pattern)`, the async-arrow-head
    analogue of the one at ~line 1429) — no code change needed, but it is a
    *behavioral* call site, not a mechanical one: it inherits the new
    rejection from `expr_to_pattern`'s `Expression::Object` arm once that
    arm is fixed in `src/parser/mod.rs`. Both this site and the one at
    ~line 1429 are gated on the following `=>` token before conversion runs,
    so e.g. `async({...x,})` as a plain call expression is unaffected —
    only `async ({...x,}) => {}` and `({...x,}) => {}` become errors.
- `src/parser/mod.rs`:
  - `expr_to_pattern`'s `Expression::Object` arm (~line 1289) — add the same
    rejection, mirroring the `Expression::Array` arm's existing
    `saw_rest && trailing_comma_after_spread` check (~lines 1282–1286),
    reusing this function's own `"Rest element must be last element"`
    message. This is the fix for `for-in`/`for-of` LHS
    (`for ({...rest,} in x) ;` / `for ({...rest,} of x) ;`) and, as a
    correct side effect, for arrow-function parameter lists that reinterpret
    an object literal as a binding pattern (`expr_to_pattern` is shared by
    both call sites; the binding grammar has the same "no comma after rest"
    shape, so no divergent behavior is introduced).
  - `pattern_to_expr` (~line 1360/1390) — update the `Expression::Object(...)`
    construction to pass `false` for the new field (synthesized node, never
    has a source-level trailing comma), matching how it already passes
    `false` when rebuilding `Expression::Array`.
  - Three `Expression::Object(props)` match arms used for other checks
    (~lines 544, 671, 765) become `Expression::Object(props, _)`.
- `src/parser/declarations.rs` — one `Expression::Object(props)` match arm
  (~line 649) becomes `Expression::Object(props, _)`. No other change; this
  file's own `parse_object_pattern` (binding patterns) is already correct.
- `src/interpreter/eval.rs` — `Expression::Object(props)` match arms
  (~lines 595, 3316, 4184) become `Expression::Object(props, _)`; the
  construction at ~line 4144 gets `, false`.
- `src/interpreter/mod.rs` — one match arm (~line 4178) becomes
  `Expression::Object(props, _)`.
- `src/interpreter/generator_transform.rs` — match arm (~line 1087) becomes
  `Expression::Object(props, _)`; the two constructions (~lines 1135, 2385)
  get `, false` (these are synthesized rewrites of destructuring targets for
  generator replay, never source-level trailing commas).
- `src/interpreter/generator_analysis.rs` — three match arms (~lines 463,
  702, 756) become `Expression::Object(props, _)`.

No `docs/` changes: this is a narrow grammar-conformance fix, not an
architectural decision, and introduces no new vocabulary.

## 4. TDD slices

1. **Thread the trailing-comma bit through `Expression::Object` (plumbing).**
   Change the AST variant and `parse_object_literal` to compute and store it,
   following `parse_array_literal`'s existing logic exactly. Add a unit test
   in `src/parser/mod.rs`'s `mod tests` (or `src/parser/expressions.rs` if it
   has its own) that parses `"({...x,});"`, `"({...x});"`, and
   `"({a, ...x});"` as expression statements and asserts the boolean on the
   resulting `Expression::Object` is `true`, `false`, `false` respectively.
   This test fails to compile before the field exists (red) and passes once
   `parse_object_literal` sets it correctly (green). Fix up every other
   `Expression::Object` match/construction site so the crate compiles
   (`cargo build --release`); these sites are behavior-preserving by
   construction (compiler-enforced exhaustiveness), so no additional test is
   needed for them.
2. **Reject the trailing comma in plain assignment expressions.** Add a test
   asserting `Parser::new("0, {...rest,} = {};").unwrap().parse_program()`
   `.is_err()` — red against current code (this is the issue's own repro).
   Add mirrored `.is_ok()`/`.is_err()` assertions that must stay correct
   throughout, chosen to pin the flag to "comma directly follows a spread
   property" rather than "a spread and a trailing comma both occur somewhere
   in the object":
   - `.is_ok()`: `"0, {...rest} = {};"`, `"0, {a, ...rest} = {};"`,
     `"var o = {...rest,};"` (plain object literal, not a pattern), and
     `"({a, b,} = {});"` (trailing comma with no rest at all — the
     discriminating case: it must not be enough for the object to merely
     *contain* a spread and *end in* a comma).
   - `.is_err()`: `"({a: {...r,}} = {});"` and `"([{...r,}] = []);"`, to pin
     that the check is reached through recursion for nested patterns, not
     only at the top level.
   Make it pass by adding the trailing-comma check to
   `validate_destructuring_pattern`'s `Expression::Object` arm in
   `src/parser/expressions.rs`.
3. **Reject the trailing comma in `for-in`/`for-of` heads and arrow
   parameters.** Add tests asserting `"for ({...rest,} in obj) ;"` and
   `"for ({...rest,} of obj) ;"` are `.is_err()` — red (these are the other
   two failing test262 fixtures). Add mirrored `.is_ok()` assertions for
   `"for ({...rest} in obj) ;"` and `"for ({...rest} of obj) ;"`. Since
   `expr_to_pattern` is also reused for arrow-parameter reinterpretation
   (§3), add `.is_err()` for `"({...r,}) => {};"` alongside `.is_ok()` for
   `"({...r,});"` (same source text, not followed by `=>` — must stay a
   valid plain parenthesized expression) and for `"({...r}) => {};"`. Make
   it pass by adding the same check to `expr_to_pattern`'s
   `Expression::Object` arm in `src/parser/mod.rs`.
4. **Run the exact test262 regressions named in the issue** to confirm they
   now fail to parse as expected (test262 "negative/parse" tests pass when
   the engine *rejects* the source):
   ```
   uv run python scripts/run-test262.py test262/test/language/expressions/assignment/dstr/obj-rest-before-comma-invalid.js
   uv run python scripts/run-test262.py test262/test/language/statements/for-in/dstr/obj-rest-before-comma-invalid.js
   uv run python scripts/run-test262.py test262/test/language/statements/for-of/dstr/obj-rest-before-comma-invalid.js
   ```

## 5. Test surface

Targeted test262 directories to run after the fix (regression + the three
named fixtures):

- `test262/test/language/expressions/assignment/dstr/` — the primary
  assignment-expression destructuring suite; exercises the
  `validate_destructuring_pattern` path broadly (object/array, rest/no-rest,
  nested, defaults) so a regression in the shared `Expression::Object` arm
  would show up here.
- `test262/test/language/statements/for-in/dstr/` and
  `test262/test/language/statements/for-of/dstr/` — exercise the
  `expr_to_pattern` path for statement heads.
- `test262/test/language/expressions/arrow-function/` — `expr_to_pattern` is
  shared with arrow-parameter reinterpretation; must confirm no valid arrow
  parameter list (object rest without trailing comma) regresses.
- `test262/test/language/expressions/object/` — plain `ObjectLiteral`
  parsing (not pattern position); confirms `{...x,}` is still accepted as an
  expression, since the new check only fires during pattern reinterpretation.
- `test262/test/built-ins/GeneratorFunction/`,
  `test262/test/built-ins/AsyncGeneratorFunction/`, and any
  generator/async-generator destructuring-in-body tests — the
  `Expression::Object` construction sites touched in
  `generator_transform.rs` are on the generator-replay desugaring path.
- Full suite: `uv run python scripts/run-test262.py` to confirm zero
  regressions against the baseline read from `origin/main:test262-pass.txt`,
  followed by the repo-mandated `README.md` pass-count/percentage update
  (not the `--update-baseline` file, which stays a `main`-only operation).

No new `test262-extra/` test is needed: the change is a narrow grammar
restriction and test262 already supplies exhaustive, exact coverage of both
the negative shape (the three fixtures above) and the surrounding positive
shapes (rest without trailing comma, non-rest trailing comma, plain object
literal with trailing comma after spread) via the directories listed above.

Non-engine gate: `cargo test --bin jsse` for the new unit tests in slices 1–3
plus the existing parser test suite.

This must be carried as a field on `Expression::Object` (mirroring
`Expression::Array`), not as a `self.last_obj_*`-style parser side channel
(cf. `self.last_obj_had_proto_dup`). A side channel set when
`parse_object_literal` returns would be clobbered before the consumer reads
it: for `for ({...rest,} in [{}]) ;`, the parser parses the LHS object
literal, then the `in` keyword, then the entire RHS expression, then the loop
body, and only *after* all of that calls `expr_to_pattern(expr)` on the
already-parsed LHS — any single most-recent-literal flag would reflect the
RHS or body's own object literals (if any) by the time `expr_to_pattern`
reads it, not the LHS's. The field survives on the `Expression::Object` node
itself; a side channel does not.

## 6. Regression risk

- **AST shape change fans out widely.** `Expression::Object` gains a
  positional field, touching ~19 match/construction sites across `ast.rs`,
  `parser/{expressions,mod,declarations}.rs`, and
  `interpreter/{eval,mod,generator_transform,generator_analysis}.rs`. The
  risk is purely mechanical (a missed site fails to compile, since Rust match
  exhaustiveness is compiler-enforced) rather than a silent behavior change —
  but every construction site must be reviewed to confirm `false` is correct
  for it (synthesized nodes never carry a source-level trailing comma).
- **Two independent enforcement points must both be fixed.** Fixing only
  `validate_destructuring_pattern` (assignment expressions) would leave the
  `for-in`/`for-of` test262 fixtures red, and vice versa — they are separate
  functions with separate `Expression::Object` arms, not one shared
  validator.
- **Must not over-reject.** `{...x,}` is valid as a plain `ObjectLiteral`
  expression (e.g. `let o = {...x,};`) and must keep parsing; the check must
  fire only inside `validate_destructuring_pattern` / `expr_to_pattern`
  (i.e., only when the object literal is being reinterpreted as a pattern),
  never inside `parse_object_literal` itself.
- **Shared tree-walker/eval paths are untouched at runtime.** The fix is
  entirely parse-time (grammar rejection); `eval.rs`'s
  `destructure_object_assignment` and `eval_object_literal` keep their
  current runtime semantics — they only need the mechanical `_`/`false`
  update to keep compiling.
- **`generator_transform.rs`'s synthesized `Expression::Object` nodes** must
  not accidentally acquire `true` for the new field (they are constructed by
  merging/rewriting property lists, not by re-parsing source text) — passing
  `false` explicitly at both construction sites avoids resurrecting a
  trailing-comma flag from unrelated source positions.
- **Bytecode fast path (`bytecode/`) and GC (`gc.rs`)** are not implicated:
  this changes `Expression`/`Pattern` construction at parse time only, before
  any bytecode compilation or GC-managed object exists.

## 7. Out of scope

- Any refactor of the duplication between `validate_destructuring_pattern`
  and `expr_to_pattern` (e.g., unifying them into one destructuring
  validator). They currently diverge in more ways than this one check (error
  message wording, strict-mode identifier checks, getter/setter rejection);
  unifying them is a larger, separate refactor and is not needed to close
  this issue.
- Revisiting the deleted upstream `array-rest-elision-invalid` dstr tests
  mentioned in the issue notes — they were removed upstream, not added, and
  jsse's array-side handling already passes the replacement coverage.
- Any change to `parse_object_pattern` / `parse_array_pattern` in
  `src/parser/declarations.rs` (binding patterns) — both already reject
  trailing comma after rest correctly and are not part of this issue.
- Rolling `test262-pass.txt` forward via `--update-baseline` — that stays a
  `main`-branch operation per repo convention.
