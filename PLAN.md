# Plan: issue #608 — a scoping combinator for the parser's context re-scoping sites

## 0. Retarget notice (read this first)

The issue body (still unedited) proposes a `SavedContext` snapshot struct with manually
invoked `save_context()`/`restore_context()`. **That proposal was superseded before any
implementation existed**, by a comment from the repo owner on this issue
(`gh issue view 608 --comments`, posted 2026-09-05T21:52:29Z, heading "Retarget: a scoping
combinator, not a snapshot struct"):

> a `SavedContext` snapshot you still restore by hand is one notch too shallow — it removes
> the duplicated fields but leaves "forgot to restore on the error path" reachable. A
> combinator that owns the restore point removes the bug class structurally.

This workspace was reused from a prior attempt. Commits `071046b..7471b11` (2026-09-06,
13:24–13:38, i.e. **after** the retarget comment) implemented the *original*, superseded
snapshot-struct design anyway: `SavedBlockScope`/`SavedFunctionContext` plus
`save_*`/`restore_*` method pairs, invoked by hand at each of the seven sites as
`let saved = self.save_x(); <mutate fields>; let result = self.body_fn(); self.restore_x(saved);
result?`. A second planning-stage run then verified that implementation as complete
(`cargo test`, lint, targeted test262 all green) without checking the issue comments, and
committed that verification as `02bef91`. Both runs missed the retarget.

Verified facts about the current branch state (checked this run):
- `cargo test --lib parser::` and `./scripts/lint.sh` are green; targeted test262 passed
  10,089/10,089 with 0 regressions (figures from the `02bef91` verification pass, spot-checked
  by re-grepping the code below rather than re-run, since nothing has changed since).
- Every one of the seven sites already restores unconditionally (result captured in a local
  *before* the restore call, restore called, *then* `?`/propagation) — confirmed by reading
  `src/parser/statements.rs:228-236` (block), `:1320-1390`-ish (try/catch/finally),
  `:1441-1460`-ish (switch-case), and `src/parser/declarations.rs:757-785` (static block),
  `:1261-1299` (`parse_function_body_inner`). So the *literal* #597 bug (leak on error path) is
  already fixed at all seven sites today. What's missing is the retargeted issue's actual ask:
  closing the bug class *structurally*, so a ninth future site can't reintroduce it by copying
  the wrong idiom.
- `save_block_scope`/`restore_block_scope`/`save_function_context`/`restore_function_context`
  (`src/parser/mod.rs:315-372`-ish) are reusable as-is — they are exactly the "snapshot" half of
  a combinator. Nothing here needs to be thrown away; it needs a combinator wrapped around it so
  call sites can no longer invoke save/restore separately.

This plan supersedes `PLAN.md`'s prior revisions on this branch. It targets the retargeted
design. The next stage should treat commits `071046b..7471b11`/`02bef91` as a real, working
intermediate state to build on (not to revert), and convert each of the seven call sites from
the hand-rolled save/mutate/body/restore/propagate idiom to the combinator below.

## 1. Problem restated

Seven call sites in the recursive-descent parser re-scope a subset of `Parser`'s context flags
around a nested parse. Today each site (already, per the prior implementation on this branch)
captures a snapshot, mutates fields, runs the nested parse, and restores the snapshot
unconditionally before propagating any error — which is correct, but only because each site's
author got the idiom right by hand. The retargeted ask is to make that idiom impossible to get
wrong: introduce a combinator that *owns* the save point, the restore point, and the
unconditional-restore-before-propagate ordering, taking the "enter" mutation and the nested
parse as two closures. Each of the seven sites becomes a single call to
`self.with_block_scope(enter, body)` or `self.with_function_context(enter, body)`; there is no
longer a restore call for a future author to accidentally omit, reorder, or gate on `Ok`.

## 2. Spec basis

Same as before — this is an internal refactor of parser bookkeeping shape, not of the JavaScript
behavior it implements. No syntax or semantics change for any program that parses successfully
or unsuccessfully today. The scoping semantics being preserved are governed by:

- **Block** — §14.2 *The `Block` Statement* / §14.2.1 (Static Semantics: Early Errors),
  `sec-block` — cited inline at `src/parser/statements.rs:247`-ish
  (VarDeclaredNames/LexicallyDeclaredNames overlap).
- **`try` Statement** — §14.15 *The `try` Statement* and its CatchParameter early-error clause
  (no duplicate bindings, no overlap with the catch block's LexicallyDeclaredNames).
- **`switch` Statement** — §14.12 *The `switch` Statement* / §14.12.1 (CaseBlock
  VarDeclaredNames/LexicallyDeclaredNames overlap), plus the CaseClause/DefaultClause
  `in_switch_case` scoping.
- **Class static initialization blocks** — `sec-class-definitions-static-semantics-early-errors`
  (verified present in `spec/spec.html`) and the `ClassStaticBlock`/`ClassStaticBlockStatementList`
  productions (verified present, e.g. `spec/spec.html:7752`) — no `arguments`, no `super()`,
  restricted `return`, own `[[HomeObject]]`; `ClassStaticBlockStatementList` has no
  DirectivePrologue production, which is why `strict` stays untouched by the static-block site.
- **FunctionBody / FormalParameters** — §15.2.1 (a `let`/`const` bound name in FunctionBody must
  not also be a FormalParameter name) governs `function_param_names`.
- **Annex B**, `sec-block-level-function-declarations-web-legacy-compatibility-semantics`
  (verified present in `spec/spec.html`) governs `in_block_or_function`/`in_switch_case`.

None of these clauses change, and none of the seven sites' field-mutation lists change either —
this plan only changes *how* the save/mutate/restore/propagate sequence is invoked, not *which*
fields each site touches. A wrong field ending up inside `with_block_scope`'s or
`with_function_context`'s `enter` closure at a given site would be a silent regression against
one of these clauses, exactly as it would have been under the hand-rolled version.

## 3. Files to touch

- `src/parser/mod.rs` — add two combinator methods on `impl<'a> Parser<'a>`:
  - `fn with_block_scope<T>(&mut self, enter: impl FnOnce(&mut Self), body: impl FnOnce(&mut Self) -> Result<T, ParseError>) -> Result<T, ParseError>`
  - `fn with_function_context<T>(&mut self, enter: impl FnOnce(&mut Self), body: impl FnOnce(&mut Self) -> Result<T, ParseError>) -> Result<T, ParseError>`
  Each: snapshot via the existing `save_block_scope`/`save_function_context`, call `enter(self)`,
  call `body(self)` capturing the result, call the existing `restore_block_scope`/
  `restore_function_context` unconditionally, then return the captured result (propagation is
  the caller's `?`, not the combinator's). Add a focused unit test for each combinator proving
  restore happens on both the `Ok` and `Err` body paths (new tests, not a modification of the
  existing `truncated_source_restores_context_counters`, which stays as an end-to-end guard).
- `src/parser/statements.rs` — convert `parse_block_statement` (~:228), the try/catch/finally
  arms of `parse_try_statement` (~:1320-1390), and the switch `CaseBlock` loop's
  `in_switch_case` site (~:1441-1460) from the hand-rolled `let saved = ...; result; restore;
  result?` shape to `self.with_block_scope(|p| { ... }, |p| { ... })?`.
- `src/parser/declarations.rs` — convert the class-static-block arm of `parse_class_element`
  (~:757-785) and `parse_function_body_inner` (~:1261-1299) to
  `self.with_function_context(|p| { ... }, |p| { ... })`, keeping `saved_param_names`,
  `prev_strict`, and `function_param_names` handling *outside* the combinator call exactly as
  today (captured before, restored after) — these three are deliberately excluded from
  `SavedFunctionContext` per the issue's own "Care needed" section and that exclusion doesn't
  change.
- No `docs/adr/` entry: same reasoning as before — this is a mechanical strengthening of
  existing internal bookkeeping, not a new architectural decision (the combinator shape was
  specified by the issue owner in-thread, not chosen among competing designs here).

## 4. TDD slices

1. **Red → green, combinator in isolation:** in `src/parser/mod.rs`'s test module, add a test
   that calls `with_block_scope` with an `enter` closure that flips `in_block_or_function` and
   `in_switch_case`, and a `body` closure that returns `Err(...)` unconditionally; assert both
   fields are back to their pre-call values after the call returns, and that the `Err` came
   through. Add the mirror-image `Ok`-path test. Both fail to compile until the combinator
   exists (red), then pass once it's implemented (green). Repeat for `with_function_context`
   with a couple of its fields (e.g. `in_generator`, `in_block_or_function`) — no need to cover
   every field, this test is about the combinator's control flow, not field completeness.
2. **Green, mechanics:** implement `with_block_scope`/`with_function_context` in
   `src/parser/mod.rs` per §3. No call sites changed yet. Slice 1's tests go green;
   `truncated_source_restores_context_counters` and the full parser test module still pass
   unchanged (nothing calls the new methods yet, so this slice is additive-only).
3. **Convert `parse_block_statement`** (`statements.rs:228`) to `with_block_scope`. Move the
   `enter` mutations (none today beyond what `parse_block_statement_body` needs — check whether
   this site currently mutates anything before calling its body helper, or whether it relies on
   ambient state; if `enter` is a no-op here keep it as `|_p| {}` rather than inventing
   mutations) and pass `parse_block_statement_body` as `body`. Delete the local `let saved =
   ...; let result = ...; self.restore_block_scope(saved); let stmts = result?;` sequence.
   Re-run the full parser test module.
4. **Convert the try/catch/finally arms** one at a time (try-block, then catch-block, then
   finally-block), same mechanical transform, re-running tests after each so a mistake in one
   arm is isolated to one commit.
5. **Convert the switch-case site** (`CaseBlock` loop, ~`statements.rs:1441`) the same way.
6. **Convert the class-static-block arm** (`declarations.rs:757`) to `with_function_context`:
   the ten-field mutation block at `:760-770` becomes the `enter` closure, `|p|
   p.parse_static_block_statements()` becomes `body`. Re-run tests; specifically re-check the
   `'arguments' is not allowed in class static blocks` post-check at `:780-782` still runs after
   the combinator call returns (it must stay outside the combinator — it inspects the *parsed
   statements*, not parser state, and doesn't need scope restored first, but restoring first is
   harmless and matches today's order).
7. **Convert `parse_function_body_inner`** (`declarations.rs:1261`). This is the trickiest
   conversion: `saved_param_names` is taken *before* `eat(LeftBrace)`, `prev_strict` is captured
   *before* the combinator, and both are restored/consumed *after* it, exactly as today (see the
   current code at `:1268-1298` — the combinator only replaces the
   `save_function_context`/`restore_function_context` pair and the result-capture in the middle,
   nothing about the surrounding param-name/strict handling changes). The `body` closure is
   `|p| p.parse_function_body_statements(saved_param_names.as_ref()).and_then(|body| {
   p.eat(&Token::RightBrace)?; Ok(body) })`. Re-run the full parser test suite plus
   `truncated_source_restores_context_counters` explicitly.
8. **Cleanup:** once no call site outside the two combinators invokes `save_block_scope`,
   `restore_block_scope`, `save_function_context`, or `restore_function_context` directly,
   confirm with `grep -n "save_block_scope\|restore_block_scope\|save_function_context\|restore_function_context" src/parser/*.rs`
   that each of the four has exactly one call site (inside its combinator). Run
   `./scripts/lint.sh` to confirm no dead code / unused-result warnings.
9. **Full-suite gate:** `cargo test --release`, then the targeted test262 directories in §5,
   then the full `uv run python scripts/run-test262.py` to confirm the baseline holds (no
   `--update-baseline`).

Convert one site per commit, re-running the test suite after each, for the same reason the
prior plan gave: a conversion that silently changes which fields a site's `enter` closure
touches is exactly the class of bug this issue exists to prevent.

## 5. Test surface

Targeted test262 directories (no observable behavior change expected):

- `test262/test/language/statements/block/`
- `test262/test/language/statements/try/`
- `test262/test/language/statements/switch/`
- `test262/test/language/statements/class/` (static-init-block early errors:
  `static-init-invalid-return.js`, `static-init-invalid-arguments.js`,
  `static-init-scope-var-derived.js`, and siblings matching `static-init-*.js`)
- `test262/test/annexB/language/statements/function/` (Annex B.3.3 block-level function
  declarations, the `in_block_or_function`/`in_switch_case` consumer)

No new `test262-extra/` file: as before, the gap this issue closes (a structural guarantee
about parser-internal bookkeeping, not an observable spec behavior) isn't test262's job to
cover. It's covered by:
- The existing `truncated_source_restores_context_counters` test in `src/parser/mod.rs`
  (already extended, per the prior implementation, to assert `in_block_or_function` and
  `in_switch_case`) — keep it as the end-to-end guard.
- The two new combinator-level unit tests from TDD slice 1 — these are the tests that actually
  exercise the structural guarantee the retarget asked for (restore-on-`Err`, independent of any
  particular parser construct).

## 6. Regression risk

- **`enter`-closure field drift.** Converting a site means moving its mutation block verbatim
  into a closure. The risk is dropping or adding a field mutation during the move — diff each
  converted site's `enter` closure against its pre-conversion mutation list line-for-line, not
  just by re-running tests (the existing tests don't cover every field at every site).
- **Order-sensitive surrounding code.** `parse_function_body_inner` and the static-block arm
  both have logic that must stay *outside* the combinator call (param-name/strict handling;
  the `'arguments'`-in-static-block post-check). Moving any of that *inside* an `enter` or
  `body` closure would change when it runs relative to the scope restore — see slice 6/7 above
  for the exact ordering to preserve.
- **Closure borrow shape.** `body` closures that need to call further `&mut self` methods (e.g.
  `p.eat(&Token::RightBrace)?` inside `parse_function_body_inner`'s `body`) take `p: &mut Self`
  as their own parameter rather than capturing `self` — this is what avoids the borrow-checker
  conflict that the issue said ruled out an RAII guard. If a conversion accidentally tries to
  capture `self` by reference inside a closure instead of using the closure's own parameter, it
  won't compile — that's a compile-time backstop, not a silent risk, but worth calling out so
  the implementer doesn't fight the borrow checker by reintroducing the RAII shape the issue
  already rejected.
- **Shared machinery leaned on:** none of the tree-walker, `property.rs` MOP, GC rooting,
  `ObjectKind` matches, bytecode fast path, or Node-compat library harnesses are touched — this
  is parser-only. The only shared risk is `test262-pass.txt`; per project convention this plan
  does not roll the baseline forward regardless of outcome.

## 7. Out of scope

The retarget comment also listed three "also in scope for the same pass" items and a separate
`in_non_arrow_function`-normalization item. This plan deliberately does **not** bundle them into
the same PR as the combinator conversion, for reasons noted next to each — but names them so
they aren't lost:

- **`reject_var_lexical_collision` dedup** (four near-identical VarDeclaredNames/
  LexicallyDeclaredNames collision checks at `statements.rs:247`-ish, `statements.rs:1448`-ish,
  `declarations.rs:1266`-ish, `mod.rs:936`-ish, two of which build `ParseError` by hand instead
  of using `self.error()`). Independent of the combinator shape — worth its own small PR/issue
  since it also fixes an error-position inconsistency, which deserves its own review and test
  coverage rather than riding along.
- **Consolidating `parse_static_block_statements` into `parse_block_statement`'s shared body
  helper**, including adding the missing `&& self.current != Token::Eof` loop guard. Checked
  this run: the missing guard is not a hang/panic risk (`parse_statement_or_declaration` at EOF
  already returns a parse error via the normal `eat`/`error` path, it just doesn't say
  "unterminated static block" as precisely as the block path might), so this is a quality
  improvement, not a bug fix — reasonable to defer.
- **`HashSet` swap for the O(n·m) `lexical_names.contains(name)` scan** — the owner's own comment
  frames this as "only worth doing while in there" (i.e. contingent on doing the consolidation
  above). Deferred with it.
- **Normalizing the `in_non_arrow_function` 8-vs-1 asymmetry** across eight call sites in
  `expressions.rs`/`declarations.rs` unrelated to the seven re-scoping sites this issue names.
  This is materially larger and riskier than the combinator conversion (eight sites outside the
  ones this issue is scoped to, plus a claimed behavior tightening of the regression test to
  assert unconditionally rather than only for module cases) and belongs in its own issue with
  its own plan.
- An RAII scope-guard combinator using `Drop` — still ruled out, for the reason the issue
  originally gave (fights the borrow checker) and the closure-based combinator sidesteps this
  without needing `Drop`.
- Any change to `MAX_PARSE_DEPTH`, the lexer, or AST types.
- Rolling `test262-pass.txt` forward.

A `gh issue comment` on #608 should record this scoping decision (combinator conversion now,
the four deferred items as follow-ups) so the owner can override if they'd rather have them
bundled.

## 8. Status / resume notes for the next stage

- Commits `071046b..7471b11` and `02bef91` are already on this branch, unpushed, no PR open.
  They implement the pre-retarget (snapshot-struct) design correctly and are green
  (`cargo test`, lint, targeted test262 10,089/10,089). Do not revert them; build on top —
  `SavedBlockScope`/`SavedFunctionContext`/their `save_*`/`restore_*` methods are exactly the
  snapshot half the new combinators need.
- Two automated review passes (`gh issue view 608 --comments`) already tried and bounced off
  "no PR exists yet" for this branch — expected until the next stage pushes and opens one.
- An untracked `EVIDENCE.md` may exist in this workspace from one of those bounced runs; it's
  stale once a PR exists and can be removed or left (untracked, won't ship).
- Next stage's job: implement TDD slices 1-9 above on top of the existing commits, `git rm
  PLAN.md`, post the scoping-decision comment from §7, push, and `gh pr create --base main
  --head sym/jsse/608-parser-a-scoping-combinator-for-the-seven-context-re-scoping-sites --title
  "refactor(parser): add scoping combinator for context re-scoping sites"` with a body citing
  the retarget comment and summarizing what's in/out of scope per §7.
