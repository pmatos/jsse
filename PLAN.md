# Plan: issue #608 — a scoping combinator for the parser's context re-scoping sites

## 1. Problem restated

Seven call sites in the recursive-descent parser re-scope a subset of `Parser`'s context
flags (`in_function`, `in_iteration`, `in_switch`, `in_block_or_function`, `in_switch_case`,
etc.) with a hand-written `let prev = self.field; self.field = new_value; ...parse...;
self.field = prev;` block. Six of those seven restore only on the success path today: if the
nested parse returns `Err` via `?` before the restore line runs, the mutated flags leak into
the caller. #602 fixed this shape at the two largest sites (the class-static-block arm and
`parse_function_body_inner`) by making their restores unconditional, but left the same shape
at four smaller sites (`parse_block_statement`, and the try/catch/finally bodies of
`parse_try_statement`) — its own fix comment even names the duplication out loud. This is
exactly the shape of #597: today the leak is unobservable because a parse error aborts
`parse_program` and the whole `Parser` is discarded, but any future construct that re-scopes a
counter and *doesn't* remember to capture the `Result` before restoring reintroduces a live
bug. The fix is to collapse the repeated save/mutate/restore boilerplate into two shared
snapshot types with `save_*`/`restore_*` method pairs, so restoring is a single unconditional
call at every site instead of a hand-copied block, and extend the existing regression test to
prove the four small sites no longer leak.

## 2. Spec basis

This is an internal refactor of parser bookkeeping: no JavaScript syntax or semantics changes
for any program that parses successfully today, and no error message or error/non-error
verdict changes for any program that fails to parse today (the leaked counters were never
observable — the `Parser` is always discarded on error). The refactor must preserve the exact
scoping semantics of the constructs it touches, which are governed by:

- **Block** — §14.2 *The `Block` Statement* / §14.2.1 (Static Semantics: Early Errors) governs
  `parse_block_statement`'s VarDeclaredNames/LexicallyDeclaredNames overlap check, already
  cited inline at `src/parser/statements.rs:247`.
- **`try` Statement** — §14.15 *The `try` Statement*, and §14.15.1 (CatchParameter Early
  Errors — no duplicate bindings, no overlap with the catch block's LexicallyDeclaredNames)
  cited inline at `src/parser/statements.rs:1323` and `:1348` (as §13.15.1 in this codebase's
  older numbering).
- **`switch` Statement** — §14.12 *The `switch` Statement* / §14.12.1 (Static Semantics: Early
  Errors — CaseBlock VarDeclaredNames/LexicallyDeclaredNames overlap), cited inline at
  `src/parser/statements.rs:1449`, and the CaseClause/DefaultClause `in_switch_case` scoping at
  `:1441`.
- **Class static initialization blocks** — the `ClassStaticBlock` production and its
  `sec-class-definitions-static-semantics-early-errors` clause (no `arguments`, no
  `super()`, `return` restricted, own `[[HomeObject]]`; verified present in
  `spec/spec.html` at this anchor id, section number not quoted here since it is
  render-time-computed and not present in the raw source), which is why
  `parse_class_element`'s static-block arm zeroes
  `in_function`/`in_generator`/`in_async`/`in_iteration`/`in_switch` and sets
  `in_static_block`/`allow_super_property` for the body it parses. `ClassStaticBlockBody`'s
  grammar (`spec/spec.html` near the `ClassStaticBlock` production) has no DirectivePrologue
  production, which is the spec basis for the existing choice to leave `strict` untouched
  there (see §6 below).
- **FunctionBody / FormalParameters** — §15.2.1 (Early Errors — a `let`/`const` bound name in
  FunctionBody must not also be a FormalParameter name) governs `function_param_names`,
  referenced inline near `src/parser/declarations.rs:1382`.
- **Annex B**, clause id `sec-block-level-function-declarations-web-legacy-compatibility-semantics`
  (verified present in `spec/spec.html`; the "B.3.3" numbering used in older comments in this
  codebase is this same clause under an earlier edition's numbering) governs
  `in_block_or_function`/`in_switch_case`, which jointly gate whether a bare `function`
  declaration is legal directly inside a block/switch-case in sloppy mode (the
  `(!self.in_block_or_function && !self.is_module) || self.in_switch_case` check at
  `src/parser/statements.rs:8` and `:17`).

None of these clauses change. They're cited to show the refactor must reproduce, field for
field, the exact scoping each site already performs — a wrong union of saved fields would be a
silent Annex B.3.3 or static-block regression, not merely a style problem.

## 3. Files to touch

- `src/parser/mod.rs` — add `SavedBlockScope` and `SavedFunctionContext` struct definitions
  and their `save_*`/`restore_*` methods on `impl<'a> Parser<'a>`; extend the
  `truncated_source_restores_context_counters` test's `assert_counters_clean` helper.
- `src/parser/statements.rs` — convert `parse_block_statement` (~:228), the try-block,
  catch-block, and finally-block arms of `parse_try_statement` (~:1298–1388), and the
  `in_switch_case`-only site inside the switch `CaseBlock` loop (~:1441) to use
  `SavedBlockScope`.
- `src/parser/declarations.rs` — convert the class-static-block arm of `parse_class_element`
  (~:757–807) and `parse_function_body_inner` (~:1283–1342) to use `SavedFunctionContext`,
  leaving `strict`, `in_formal_parameters`, and `function_param_names` as the hand-managed
  locals they already are at whichever of the two sites touches them (see §6, "Care needed"
  carried over from the issue).
- No `docs/adr/` entry: this is a mechanical dedup of existing internal state-machine
  bookkeeping, not an architectural decision about parser design (no alternative was rejected
  other than the RAII guard the issue itself already ruled out inline).

## 4. TDD slices

1. **Red (partial — one field, not two):** extend `assert_counters_clean` in
   `src/parser/mod.rs` (`truncated_source_restores_context_counters`, currently at :1596) to
   also assert `parser.in_block_or_function == false` and `parser.in_switch_case == false` for
   every existing `SOURCES` entry. Only the first assertion is expected to go red. Trace why
   before writing it: `grep -n "in_switch_case = true" src/parser/*.rs` should return exactly
   one hit, the `CaseBlock` loop at `statements.rs:1442`, and that site already restores
   `in_switch_case` unconditionally before its own `?` (:1445) — so nothing downstream can
   observe a leaked `true`, and the field is `false` at top level regardless of today's bug.
   `in_block_or_function`, by contrast, is set `true` at six sites: the four buggy small ones
   plus the two already-fixed big ones (#602). Sources like `"for (;;) { function f() {
   for (;;) {"` and `"for (;;) { function f() { switch (x) { case 1:"` route through
   `parse_block_statement`/`parse_try_statement` and should assert `in_block_or_function`
   still `true` on current `main` — that's the red. Add the `in_switch_case` assertion anyway
   (it's the DoD's literal ask and documents the invariant), but record in the PR that it
   passes from slice 1 onward as a guard, not as evidence of a fix. No production change yet.
2. **Green (small sites, mechanics):** add `SavedBlockScope { in_block_or_function: bool,
   in_switch_case: bool }` (derive `Clone, Copy`) plus `save_block_scope`/`restore_block_scope`
   to `src/parser/mod.rs`. Convert `parse_block_statement` first. Its fallible region
   (`statements.rs:234`–`:260`) has three exit paths — the loop's own `?` at :238, the
   `collect_lexical_names_with_func_names(...)?` at :239, and the explicit `return Err` at
   :255 — so "capture the result in a local" means extracting that region into a helper
   function returning `Result<Vec<Statement>, ParseError>` (the same shape
   `parse_static_block_statements`/`parse_function_body_statements` already use at the two big
   sites), then at the call site: `let r = self.helper(); self.restore_block_scope(saved); r`
   (propagate with `?` only after the restore). Keep `self.eat(&Token::LeftBrace)?` *before*
   `save_block_scope()`, exactly as today, so a missing `{` has nothing to restore. Re-run the
   slice-1 test; the block-only leak clears.
3. **Green (try/catch/finally):** the try/catch/finally bodies at `statements.rs:1298`–`:1388`
   share one shape — `while self.current != Token::RightBrace { block.push(self
   .parse_statement_or_declaration()?); }` — so factor that loop into a single
   `parse_statement_list_until_brace(&mut self) -> Result<Vec<Statement>, ParseError>` helper
   and call it from all three arms (plus `parse_block_statement`, which is the same loop with
   extra lexical-name bookkeeping layered on — keep that bookkeeping at the
   `parse_block_statement` call site, not inside the shared helper). Convert one arm at a
   time (try-block, then catch-block, then finally-block), re-running the test after each so a
   mistake in one arm is isolated. All four small sites now share `SavedBlockScope`.
4. **Green (switch-case, judgement call):** convert the `in_switch_case`-only site in the
   `CaseBlock` loop (~:1441) to `SavedBlockScope` too, even though slice 1 established it was
   never buggy. The issue's body enumerates six sites and says "all six sites converted," but
   its title says "seven ... re-scoping sites" and its closing goal is "no hand-written
   restore blocks left" — this site is exactly such a block. Treat migrating it as a
   judgement call: record in the PR body that it was included for the "no hand-written
   restore blocks left" goal and title-seven count, with `in_block_or_function` round-tripping
   through `SavedBlockScope` as a same-value no-op at this site. If a reviewer prefers reading
   the six-site DoD literally, this slice is the one to drop — it's independent of slices 2–3
   and 5–6. No behavior change either way; no new test needed beyond confirming the existing
   switch sources in slice 1 still pass.
5. **Green (function-context sites):** add `SavedFunctionContext` (the fields listed in the
   issue body minus `strict`/`in_formal_parameters`, which stay local) plus
   `save_function_context`/`restore_function_context`. Convert the class-static-block arm in
   `src/parser/declarations.rs` first — it is the simpler of the two (no `strict`/
   `in_formal_parameters`/`function_param_names` handling to keep external to the struct).
   Diff the converted site's mutation list field-by-field against the original hand-written
   block (§6's first regression-risk item) rather than trusting the struct's field names to
   line up automatically. Re-run `truncated_source_restores_context_counters` plus a full
   `cargo test --release`.
6. **Green (`parse_function_body_inner`):** convert the second big site, keeping
   `saved_param_names`, `prev_strict`, and `prev_formal` as separate local saves exactly as
   today. Before converting `in_function` from its current `+= 1` / `-= 1` shape to
   `self.in_function = saved.in_function + 1` plus snapshot-restore, run
   `grep -n "\.in_function" src/parser/*.rs` and confirm every mutator restores it before
   returning control to its caller: at the time of writing this plan that grep turns up the
   static-block arm (save/restore, already unconditional post-#602), this function's own
   `+=1`/`-=1`, `parse_field_initializer_value` (`declarations.rs:1083`-ish, captures its
   result without an early `?` and always restores before returning), and
   `set_eval_in_field_initializer` (`mod.rs:267`, a one-time permanent bump for a
   dedicated eval-only `Parser` instance that is *never* restored — leave this one alone, it
   is orthogonal to this issue and does not participate in nested save/restore). If that
   grep's result set changes before this slice lands, re-verify the equivalence argument
   before proceeding — snapshot-restore is only equivalent to `-= 1` if nothing else can leave
   `in_function` altered across the save/restore window. Record the grep output and this
   reasoning in the PR description. Re-run the full parser test suite.
7. **Refactor:** delete the now-dead hand-written save/restore locals at all converted sites;
   confirm `./scripts/lint.sh` is clean (no unused `prev_*` bindings, no dead code).
8. **Full-suite gate:** run `cargo test --release`, then a targeted `test262` pass over the
   directories in §5, then the full `uv run python scripts/run-test262.py` to confirm the
   baseline holds (no `--update-baseline`).

Each slice is a single, revertible commit-sized unit: convert one site (or one struct), rerun
the extended unit test, move on. Do not convert more than one site per commit — a fix that
silently changes which fields round-trip at a given site is exactly the class of bug this
issue exists to prevent, so each conversion should be independently reviewable against its
"before" hand-written block.

## 5. Test surface

Targeted test262 directories (no spec behavior change expected — these confirm the refactor
is invisible):

- `test262/test/language/statements/block/`
- `test262/test/language/statements/try/`
- `test262/test/language/statements/switch/`
- `test262/test/language/statements/class/` (static-init-block early errors: files matching
  `static-init-*.js`, e.g. `static-init-invalid-return.js`, `static-init-invalid-arguments.js`,
  `static-init-scope-var-derived.js`)
- `test262/test/annexB/language/statements/function/` (Annex B.3.3 block-level function
  declarations, the `in_block_or_function`/`in_switch_case` consumer)

None of these need a new `test262-extra/` file: the DoD is explicit that the missing coverage
is the parser-internal counter leak on the abort path, not an observable spec gap, and that is
exactly what the existing (soon-extended) `truncated_source_restores_context_counters` unit
test in `src/parser/mod.rs` covers. `cargo test --release` is the gate for that test; test262
is the gate for "did the refactor change any observable parse result."

## 6. Regression risk

- **Field-set drift per site.** The two big sites do not save identical field sets today
  (see §2's spec citations). `SavedFunctionContext` must capture the *union* the issue
  specifies, but each site only *mutates* the subset it needs; an unmutated field round-trips
  as a same-value no-op. The risk is copying a field into a site's mutation list that the
  original hand-written block didn't touch (e.g. accidentally zeroing `in_non_arrow_function`
  in the static-block arm, which never zeroed it before) — each conversion in slices 5–6 must
  be diffed against the original block field-by-field, not just field-name-matched.
- **`labels` clone cost.** `SavedFunctionContext` carries `labels: Vec<(String, bool)>`
  because the two big sites already `std::mem::take` it today — no new cost there. Do **not**
  reuse `SavedFunctionContext` (with its `labels` field) for the four small block-like sites;
  they never touch `labels`, and cloning that `Vec` once per `{`-block parsed across all of
  test262 would be a real, avoidable allocation on the hottest part of the parser. This is why
  the plan keeps `SavedBlockScope` as a separate, deliberately smaller struct (2 `bool`
  fields, `Copy`) rather than one struct for all seven sites.
- **`strict` / `in_formal_parameters` / `function_param_names` exclusion.** Per the issue's own
  "Care needed" section, these three stay outside both shared structs and remain hand-managed
  locals at the one site that touches each. Pulling them into `SavedFunctionContext` "for
  completeness" would make the static-block arm start saving/restoring fields it has no
  behavioral need to touch, and is exactly the kind of scope creep this plan should not bundle
  in.
- **Shared machinery leaned on:** none of the tree-walker (`eval_expr`/`exec_statement`),
  `property.rs` MOP, GC rooting, `ObjectKind` matches, bytecode fast path, or Node-compat
  library harnesses are touched — this is parser-only. The only shared risk is
  `test262-pass.txt`: any directory in §5 regressing would indicate a field was
  mis-restored; per project convention this plan does **not** roll the baseline forward
  regardless of outcome (that is a `main`-branch operation).

## 7. Out of scope

- Converting any of the *other* `let prev_x = self.x; ...; self.x = prev_x;` patterns found
  during exploration (e.g. `prev_generator`/`prev_async` pairs in `expressions.rs` around
  method/getter/setter parsing, `saved_no_in` in several expression parsers, the
  lexer-position `saved_lt`/`saved_ts`/`saved_te` backtracking triples used for lookahead).
  These are a different shape (single- or dual-field, mostly already restoring unconditionally
  before their own `?`) and are not named by the issue; bundling them in would turn a
  six/seven-site targeted fix into an open-ended parser refactor.
- An RAII scope-guard combinator — explicitly ruled out by the issue itself (fights the
  borrow checker: the guard would need to hold `&mut Parser` across the whole nested body
  parse, which conflicts with the nested parse's own need for `&mut self`).
- Any change to `MAX_PARSE_DEPTH`, the lexer, or AST types.
- Rolling `test262-pass.txt` forward (`--update-baseline` is a `main`-branch operation).
- Adding a new `docs/adr/` entry (see §3 — no architectural decision is being made beyond what
  the issue itself already settled).

## 8. Status (resumed run, 2026-09-06)

This workspace was reused from a prior attempt: slices 1–8 above are already implemented and
committed on this branch (`071046b`..`7471b11`), but the branch has never been pushed and no PR
exists. This run re-verified the existing work rather than re-planning from scratch, since a
coherent `PLAN.md` was already committed and matched by the implementation.

Slice → commit map:

1. `a21cd37` — extended `truncated_source_restores_context_counters` (red for
   `in_block_or_function` on the four small sites, as predicted).
2. `0093b7b` — added `SavedBlockScope`, converted `parse_block_statement`.
3. `f551167`, `36c466c`, `2e5878f` — try-block, catch-block, finally-block arms.
4. `ced9e99` — switch-case site converted too (the "seven sites" judgement call from slice 4
   was resolved in favor of converting it).
5. `00977ce` — added `SavedFunctionContext`, converted the class-static-block arm.
6. `d4ef89d` — converted `parse_function_body_inner`.
7. `7471b11` — doc cleanup on the shared structs' field-exclusion rationale (`strict` /
   `in_formal_parameters` / `function_param_names` stay hand-managed locals; see the doc
   comment on `SavedFunctionContext` in `src/parser/mod.rs`).
8. Full-suite gate — re-run in this session, not as a separate commit (see below).

Care-needed items from the issue, verified against the landed code:

- `in_non_arrow_function` — included in `SavedFunctionContext`; restoring it at the
  static-block arm is a same-value no-op there, and the test comment at
  `src/parser/mod.rs:1720-1727` documents why it's checked via `MODULE_SOURCES`
  (`export default function`) instead of the main `SOURCES` table.
- `strict` — deliberately excluded from both structs; restored via `set_strict` (which also
  updates the lexer), documented inline on `SavedFunctionContext`.
- `in_formal_parameters` — included in `SavedFunctionContext`.
- `function_param_names` — deliberately excluded; callers reset it to `None` rather than
  restoring a prior value, documented inline on `SavedFunctionContext`.

Verification performed this run (read-only, no new production changes):

- `cargo test --lib parser::` — 15/15 pass, including
  `truncated_source_restores_context_counters`.
- `./scripts/lint.sh` — rustfmt, clippy (default and `perf-counters`) all clean.
- `grep` for hand-written `saved_*`/`prev_*` context-flag blocks in `statements.rs`/
  `declarations.rs` — none remain; the only survivors are lexer/token backtracking
  (`saved_lt`, `saved_current`, `saved_pushback`, `saved_lexer`) and the two deliberately
  hand-managed fields above, which are out of scope per §7.
- Targeted test262 (`language/statements/{block,try,switch,class,function}`): 10,089/10,089
  scenarios pass (100%), 0 regressions against `origin/main:test262-pass.txt`.
- `git fetch origin main`: no new commits on `main` since this branch's base — no rebase
  needed before push.

Remaining work for the next stage (implementation/PR-opening, not planning):

- Run the full `uv run python scripts/run-test262.py` to confirm the baseline holds
  repo-wide (only the targeted directories were run in this session).
- `git rm PLAN.md` per the stage-handoff convention.
- The untracked `EVIDENCE.md` in this workspace is a stale artifact from an unrelated `/simplify`
  run that found no PR to operate on — it documents a blocker that resolves itself once the PR
  below exists; the next stage should remove it (or leave it — it is untracked and won't ship).
- Push the branch and `gh pr create --base main --head
  sym/jsse/608-parser-a-scoping-combinator-for-the-seven-context-re-scoping-sites --title
  "refactor(parser): add scoping combinator for context re-scoping sites"` with a body
  summarizing slices 1–8 above, including the judgement call in slice 4 (switch-case site
  converted despite the issue's six-site enumeration) and the two structs used instead of the
  issue's single `SavedContext` (avoids a union-restore behavior change at the two sites whose
  saved field sets differ, per §2/§6).
