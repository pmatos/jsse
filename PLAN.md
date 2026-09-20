# Plan: issue #647 — async function expression skips strict-mode-reserved-word early error for its name

## 1. Problem restated

`parse_async_function_expression` (`src/parser/expressions.rs:2127-2180`) validates a named
`async function` / `async function*` expression's name only against `eval`/`arguments` in its
`if body_strict { ... }` block (`:2159-2168`). It omits the `Self::is_strict_reserved_word(n)`
check that `parse_function_expression` (`:2105-2109`) and `parse_function_declaration`
(`src/parser/declarations.rs:285`, which serves sync *and* async declarations) both perform.

The gap only bites when the strictness comes from the function's **own** `"use strict"`
directive. The name is parsed before the body, so `check_strict_binding_identifier`
(`src/parser/mod.rs:553`, gated on `self.strict`) cannot see it; the post-body `body_strict`
block is the only place that can catch it. Result:
`(async function package(){"use strict";})` and `(async function* package(){"use strict";})`
parse without error, while `(function package(){"use strict";})` throws SyntaxError.

## 2. Spec basis

- **Identifiers — Static Semantics: Early Errors** (`sec-identifiers-static-semantics-early-errors`),
  production `Identifier : IdentifierName but not ReservedWord`: "It is a Syntax Error if
  IsStrict(this phrase) is true and the StringValue of IdentifierName is one of "implements",
  "interface", "let", "package", "private", "protected", "public", "static", or "yield"."
  Also `BindingIdentifier : yield` — Syntax Error if IsStrict(this production).
- **Strict Mode Code** (`sec-strict-mode-code`): function code is strict if the
  associated `AsyncFunctionExpression` / `AsyncGeneratorExpression` is in strict code **or** its
  body begins with a Directive Prologue containing a Use Strict Directive. All parts of the
  function — including its `BindingIdentifier` — are therefore strict, so `IsStrict` on the name
  is true when the body has `"use strict"`.
- **Async Function Definitions — Static Semantics: Early Errors**
  (`sec-async-function-definitions-static-semantics-early-errors`) and **Async Generator
  Function Definitions — Static Semantics: Early Errors**
  (`sec-async-generator-function-definitions-static-semantics-early-errors`): the
  `BindingIdentifier` bullet ("If BindingIdentifier is present and IsStrict(BindingIdentifier) is
  true, it is a Syntax Error if the StringValue of BindingIdentifier is either "eval" or
  "arguments"") — the `eval`/`arguments` half is what the code already enforces here; the
  reserved-word half comes from §12.7.2 above via the same `IsStrict(BindingIdentifier)`.
  (Clause ids above were checked against `spec/spec.html`; cite by id/title in tests and PR.)
- Grammar parameters: `AsyncFunctionExpression` takes `BindingIdentifier[~Yield, +Await]`;
  `AsyncGeneratorExpression` takes `BindingIdentifier[+Yield, +Await]`. The parser already models
  this (`self.in_generator = is_generator; self.in_async = true` at `:2135-2136`, explicit
  `await` rejection at `:2137`), so `yield`-named async *generator* expressions are already an
  error regardless of strictness; the new check adds the strict-only words for both forms.

## 3. Files to touch

- `src/parser/expressions.rs` — add the missing `is_strict_reserved_word` check inside
  `parse_async_function_expression`'s `if body_strict { ... }` block, between the
  `eval`/`arguments` check and `self.check_strict_params(&params)?`, mirroring
  `parse_function_expression` (`:2105-2109`) including the error message
  `Unexpected strict mode reserved word '{n}'`. ~4 added lines; no signature changes.
- `test262-extra/async-function-expression-name-strict-reserved-word.js` — new test (see §4).
- No `docs/` / `CONTEXT.md` / ADR change: no new vocabulary or architectural decision.
- `spec/`, `test262/`, `test262-pass.txt` untouched.

## 4. TDD slices

Build first: `cargo build --release -j4` (no `target/release/jsse` exists in the workspace; run
with an explicit long timeout). Submodules `test262`/`spec` were initialised for reading
(`git submodule update --init --depth 1 test262 spec`); re-run if absent — needed for the
test262 runner. Run test262-extra via
`uv run python scripts/run-test262.py test262-extra/async-function-expression-name-strict-reserved-word.js`
(it has no dedicated runner).

1. **RED — async function expression, own-directive strictness.**
   Create `test262-extra/async-function-expression-name-strict-reserved-word.js` (test262
   frontmatter: `description`, `esid: sec-identifiers-static-semantics-early-errors`, `info`
   quoting the Identifiers early-error bullet and the Strict Mode Code function-code rule, `features: [async-functions]`).
   Because one negative-`phase: parse` file can only hold one snippet, use the common test262
   multi-case idiom: a table of names × forms evaluated through the `Function` constructor /
   indirect `eval` (`assert.throws(SyntaxError, () => (0, eval)(src))`), one assertion per
   case with a message naming name + form. First case group:
   `(async function NAME(){"use strict";})` for NAME in
   `implements interface let package private protected public static yield`.
   Confirm it fails on the current binary (nothing throws) for every name.
   **GREEN:** add the `is_strict_reserved_word` check in `parse_async_function_expression`.
   Re-run: all cases pass.
2. **RED→GREEN — async generator expression.** Add the same name list for
   `(async function* NAME(){"use strict";})`. `yield` is expected to throw already (BindingIdentifier
   `[+Yield]`: `current_identifier_name` returns `None` for the `yield` keyword token inside a
   generator, so parameter parsing fails); the other eight are red before and green after the
   slice-1 edit, proving `is_generator` needs no separate handling. To observe both groups red,
   author slices 1 and 2 test cases and run the file *before* making the production edit, then
   apply the one-line-block fix and re-run.
3. **Controls (guard against over-rejection, expected green from the start).** In the same file:
   - sloppy body: `(async function package(){})`, `(async function* package(){})`, and for
     `let`/`static`/`yield` (non-generator only) parse successfully (`assert.sameValue(typeof
     (0, eval)(...), "function")`);
   - non-reserved name with directive: `(async function foo(){"use strict";})` parses;
   - already-covered half of the same spec bullet, as regression coverage:
     `(async function eval(){"use strict";})` and `(async function arguments(){"use strict";})`
     throw SyntaxError;
   - already-working outer-strict path: `"use strict"; (async function package(){})` throws
     (exercises `check_strict_binding_identifier`, not the new code).
4. **Sibling-form parity check (no new production code expected).** Add cases confirming the
   sibling forms still behave identically so the test documents the invariant the issue asserts:
   `(function NAME(){"use strict";})`, `(function* NAME(){"use strict";})`, and declarations
   `async function NAME(){"use strict";}`. If any of these turns out *not* to throw, stop and
   record it in a follow-up issue rather than widening this PR (see §7).
5. **Refactor:** none. Do not extract the shared name-check helper in this PR (see §7).

Quality gates, each a separate command (never `&&`-chained): `./scripts/lint.sh`, then
`cargo test --release`, then the test262 runs in §5. Commit with a Conventional Commits message,
e.g. `fix(parser): reject strict-reserved names on async function expressions with own "use strict"`;
PR title must follow the same convention (it becomes the squash subject). `git rm PLAN.md` before
opening the PR.

## 5. Test surface

Targeted test262 runs (`uv run python scripts/run-test262.py <dir>`):
- `test262/test/language/expressions/async-function/`
- `test262/test/language/expressions/async-generator/`
- `test262/test/language/expressions/function/`
- `test262/test/language/expressions/generators/`
- `test262/test/language/statements/async-function/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/future-reserved-words/`, `test262/test/language/reserved-words/`,
  `test262/test/language/identifiers/`, `test262/test/language/directive-prologue/`,
  `test262/test/language/function-code/`, `test262/test/language/eval-code/`
- `annexB/language/function-code/` and `annexB/language/expressions/` (name-binding edge cases).
- `test262-extra/` whole directory (fast pre-flight; expected 100% green), then the full default
  suite (`uv run python scripts/run-test262.py`) since this is a parser change.
- `uv run python scripts/run-custom-tests.py` (covers `tests/strict-mode-reserved-words.js`).

Not covered by test262: a search of `test262/test/language` finds no test that names an async
function/generator *expression* with a strict-reserved word and a body-level `"use strict"`, so
the new `test262-extra/` file (slices 1-4) is the only coverage. It follows the test262 file
pattern (frontmatter + `assert.throws`) and cites the Identifiers early-error and Strict Mode
Code clauses in `info`. The
`tests/` dir is not used: the behavior is a spec-mandated early error observable as a
SyntaxError, so per project convention it belongs in `test262-extra/`.

## 6. Regression risk

- **Baseline (`test262-pass.txt`)**: expected to move by zero — upstream has no test exercising
  this gap, and the change only turns a previously-accepted program into a SyntaxError for the
  nine strict-reserved names on named async function/generator expressions whose own body has
  `"use strict"`. A test262 test that (incorrectly) relies on such a program parsing would show
  up as a regression in the targeted runs above; if one appears, the fix stays spec-correct and
  the PR notes the test as suspect rather than bending the parser. Do not run `--update-baseline`.
- **Shared machinery**: parser only (`src/parser/expressions.rs`). No interpreter, GC rooting,
  `property.rs` MOP, `ObjectKind`, or bytecode changes; tree-walker hot paths untouched.
  The `if body_strict` branch runs once per async function expression parse, so no perf impact.
- **Library harnesses**: real-world bundles (acorn, uglify-js, highlight.js, zod, luxon, moment…)
  could in principle contain `async function <reserved>` with a body directive; very unlikely and
  would already be a Node SyntaxError. A quick `./scripts/run-library-tests.sh acorn` is optional
  (cheap, ~minutes); the long ones (uglify-js, highlight.js, big.js) are not needed for a parser
  early-error tightening.
- **False-positive risk**: the check must not fire for sloppy bodies or for non-reserved names
  (covered by slice 3 controls); `is_strict_reserved_word` is the same predicate used by the two
  sibling sites, so no new word list is introduced.

## 7. Out of scope

- Extracting a shared helper for the duplicated post-body `eval`/`arguments`/reserved-word name
  checks across `parse_function_expression`, `parse_async_function_expression`, and
  `parse_function_declaration` (the root cause of the drift — worth a follow-up issue, but a
  refactor does not belong in a bug-fix PR).
- Any other divergence between `parse_async_function_expression` and
  `parse_function_expression` (e.g. the async form does not save/reset `in_static_block`, and
  calls `check_duplicate_params_strict` unconditionally). Unverified as bugs and not part of
  #647; file separately if confirmed.
- Escaped-`await` names and other `await`-as-BindingIdentifier edge cases (`:2137` only checks the
  unescaped keyword token).
- Formatting or comment changes beyond the added lines; no `test262-pass.txt` update; no changes
  to `spec/` or `test262/`.
