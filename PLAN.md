# Plan: issue #608 — a scoping combinator for the parser's context re-scoping sites

## 0. State of the branch (read this first)

The implementation is **already on this branch and verified green locally**. What has never
happened is the *delivery*: the branch was never pushed and no PR exists
(`gh pr list --head <branch> --state all` → `[]`; `git ls-remote --heads origin` → nothing).
Two later pipeline stages (code-review, simplify) bounced off exactly that and commented on the
issue. The remaining job of the next stage is therefore **finish verification, delete this
file, push, open the PR** — not to re-implement anything.

**Push early.** Prior implementation runs finished their code but ended before pushing. The
first action of the next stage should be `git rm PLAN.md` + commit + `git push -u origin HEAD` +
`gh pr create` (see §8); the long test262 run comes *after* the PR exists, and its result goes in
a PR comment. Nothing about a green targeted test262 run changes what gets pushed.

### Design (retargeted by the owner's comment)

The issue body proposes a `SavedContext` snapshot with hand-invoked `save_context()`/
`restore_context()`. The owner's comment on the issue ("Retarget: a scoping combinator, not a
snapshot struct", 2026-09-05) supersedes that: a snapshot you restore by hand still lets "forgot
to restore on the error path" be written, so the restore point must be owned by a combinator.
That is what is on the branch:

- `SavedBlockScope { in_block_or_function, in_switch_case }` and `SavedFunctionContext { 13
  fields incl. labels }` — private snapshot structs in `src/parser/mod.rs`.
- `fn with_block_scope<T>(&mut self, enter: impl FnOnce(&mut Self), body: impl FnOnce(&mut
  Self) -> Result<T, ParseError>) -> Result<T, ParseError>` and the `with_function_context`
  twin (`src/parser/mod.rs:393`, `:408`): snapshot → `enter` → `body` → **unconditional
  restore** → return the captured `Result` for the caller's `?`.
- `save_*`/`restore_*` are now called only from inside the two combinators (verified by grep this
  run: zero direct calls in `statements.rs`/`declarations.rs`).
- `Parser::enter_block_scope` — the `enter` closure shared by the four brace-body sites
  (`in_block_or_function = true; in_switch_case = false`).

### Slice → commit map (all done)

| Slice | Commit(s) |
|---|---|
| Extend `truncated_source_restores_context_counters` to assert `in_block_or_function`/`in_switch_case` (issue DoD) | `a21cd375` |
| Snapshot structs + `parse_block_statement`, try / catch / finally arms, switch-case site | `0093b7b4`, `f5511676`, `36c466c4`, `2e5878f7`, `ced9e990` |
| Class static block, `parse_function_body_inner` | `00977ce5`, `d4ef89d7` |
| Doc on the counter fields | `7471b11f` |
| Combinators + their four unit tests (`with_{block_scope,function_context}_restores_on_{ok,err}`); all seven sites moved onto them | `e85c17f8` |
| Fix: `with_block_scope` must not force `in_block_or_function = true` (the switch consequent needs it left ambient; forcing it had made `using` directly in a `case` body parse) — each site's `enter` now sets exactly its own fields, regression test `using_declaration_rejected_directly_in_switch_case` | `5de5b202` |

`5de5b202` is worth a reviewer's attention: it is the one place a combinator conversion changed
behavior, and it was caught and fixed on the branch. The PR description should say so.

Verified this run (2026-09-21): `cargo test --lib parser::` → 20/20 pass (incl. the four
combinator tests, `truncated_source_restores_context_counters`,
`using_declaration_rejected_directly_in_switch_case`); `./scripts/lint.sh` → rustfmt, clippy
`-D warnings`, clippy `perf-counters` all clean.

**Not** re-verified since the combinator commits: targeted/full test262. The earlier 10,089/10,089
figure predates `e85c17f8`/`5de5b202` and must not be cited. It has to be re-run (§4).

## 1. Problem restated

Seven parser sites re-scope context flags around a nested parse by a hand-written
save → mutate → restore block. #597 was a restore that ran only on the success path; #602 fixed
the first two sites but left the shape in place, so the other sites' correctness depended on each
author remembering to capture the `Result` before restoring. The combinator owns the save point,
the restore point and the restore-before-propagate order, so a future site cannot reintroduce
#597 by writing the wrong idiom, and the four smaller sites stop leaking `in_block_or_function`/
`in_switch_case` on an aborted parse.

## 2. Spec basis

N/A: no JavaScript behavior change. This is an internal refactor of parser bookkeeping; every
program parses or is rejected exactly as before. (The one field-set difference the issue asked to
be argued — see §6 — is a proven no-op, and the accepted/rejected language is pinned by test262.)
Clauses whose behavior the flags encode, for the reviewer:
§14.2.1 *Block* early errors; §14.15 *try* (CatchParameter early errors); §14.12.1 *switch*
CaseBlock early errors; §15.7.1 *ClassStaticBlock* early errors (no `arguments`, no `super()`,
`ClassStaticBlockStatementList` has no DirectivePrologue); §15.2.1 FunctionBody vs
FormalParameters names; Annex B.3.3 block-level function declarations (consumer of
`in_block_or_function`/`in_switch_case`).

## 3. Files touched (already)

`src/parser/mod.rs` (structs, combinators, `enter_block_scope`, unit tests, field doc),
`src/parser/statements.rs` (block, try/catch/finally, switch-case sites),
`src/parser/declarations.rs` (class static block, `parse_function_body_inner`). No `docs/adr/` or
`CONTEXT.md` change: no new architectural decision (the combinator shape was specified by the
owner in-thread) and no new domain vocabulary. `PLAN.md` itself is deleted before the PR.

## 4. Remaining work for the next stage (in order)

1. **Deliver first.** `git rm PLAN.md`; commit `chore: drop plan doc` (or fold into the PR's
   final commit — the PR is squash-merged, so subject wording is irrelevant except the PR title);
   `git fetch origin main`; if `main` moved, `git rebase origin/main` and re-run step 2's cargo
   commands; `git push -u origin HEAD`; `gh pr create --base main --head
   sym/jsse/608-parser-a-scoping-combinator-for-the-seven-context-re-scoping-sites --title
   "refactor(parser): add scoping combinators for context re-scoping sites" --body-file
   <path under $TMPDIR>`. Body: closes #608; the retarget-to-combinator rationale; the `5de5b202`
   behavior fix; the §6 no-op argument; the §7 deferrals; and "test262: results in a follow-up
   comment" until §4.3 finishes.
2. **Quality gates, separate commands, not `&&`-chained** (repo rule), `-j 4` cap:
   `cargo test --lib parser::`, then `cargo test --release`, then `./scripts/lint.sh`.
3. **test262.** Fresh workspaces have empty submodules: `git submodule update --init --depth 1
   test262` first. `cargo build --release -j 4`, snapshot the binary before any run (do not
   rebuild mid-run), then targeted:
   `uv run python scripts/run-test262.py test262/test/language/statements/{block,try,switch,class}/`
   and `.../test262/test/annexB/language/`, `.../language/expressions/class/`,
   `.../language/function-code/`, `.../language/statements/function/` and the
   `language/{directive-prologue,block-scope,eval-code}` dirs; then the full default run. No
   `--update-baseline`; the runner compares against `origin/main:test262-pass.txt`. Any
   regression is a bug in a site's `enter` closure — diff it against the pre-conversion mutation
   list (`git diff a21cd375..HEAD -- src/parser/` shows every site before vs. after conversion).
   Post the result as a PR comment.
4. Post a short `gh issue comment 608` (or PR comment) recording the scoping decision in §7.

## 5. Test surface

- Unit (in `src/parser/mod.rs`, all present): the four combinator tests prove restore on both the
  `Ok` and `Err` body paths independent of any grammar construct;
  `truncated_source_restores_context_counters` is the end-to-end guard and now asserts
  `in_block_or_function` and `in_switch_case` (issue DoD); `using_declaration_rejected_directly_in_switch_case`
  pins the `5de5b202` regression.
- test262 (targeted, §4.3): `language/statements/{block,try,switch,class}`, `annexB/language`
  (B.3.3 consumer), `language/statements/function`, `language/expressions/class`,
  `language/function-code`.
- No new `test262-extra/` file: the guarantee is parser-internal bookkeeping with no observable
  ECMAScript behavior of its own; anything observable is already in test262.

## 6. Regression risk

- **Field-set differences at the two big sites (the issue's "Care needed").** Argued, not
  assumed:
  - `parse_function_body_inner` now round-trips `in_non_arrow_function`. Its inner parse never
    mutates it (callers bracket the call), so restoring the saved value is a same-value store.
  - The static-block site now round-trips `in_formal_parameters` but its `enter` closure does
    **not** touch it (unlike `parse_function_body_inner`, which sets it `false`). Only the
    formal-parameter parser sets it (`declarations.rs:1182`, restored at `:1206`), and a static
    block's statement list never leaves it changed, so the round-trip is a same-value store and
    behavior is identical to the pre-conversion code. (Pre-existing quirk, out of scope: a static
    block inside a parameter default keeps `in_formal_parameters == true` in its body.)
  - `strict` and `function_param_names` are **deliberately excluded** from
    `SavedFunctionContext`. `strict` goes through `set_strict` (which also updates the lexer) and
    stays a hand-managed local at `parse_function_body_inner`; the static-block arm never runs a
    directive prologue (no DirectivePrologue in `ClassStaticBlockStatementList`), so `strict` is
    untouched there. `function_param_names` is taken before and reset/restored after at
    `parse_function_body_inner` exactly as before, and the static-block arm keeps not touching
    it — the careful item in the issue is preserved by *not* widening the snapshot.
- **`enter`-closure field drift**: moving a mutation list into a closure can drop or add a field.
  `5de5b202` is a real instance (the switch consequent). The full test262 run is the backstop.
- **Closure borrow shape**: `enter`/`body` take `&mut Self` as their parameter, never capture
  `self`; compile-time enforced.
- **Shared machinery**: parser only. No tree-walker, `property.rs`, GC, `ObjectKind`, bytecode or
  library-harness change. Only `test262-pass.txt` could move; it is not rolled forward here.

## 7. Out of scope (named so they aren't lost; file as follow-ups)

Listed in the owner's retarget comment as "also in scope", deliberately **not** bundled: they are
independent of the combinator, and one of them (error-position consistency) deserves its own
review.

- **`reject_var_lexical_collision` dedup** — four near-identical VarDeclaredNames/
  LexicallyDeclaredNames checks (§14.2.1, §14.12.1, static block, §16.1.1); two build
  `ParseError` by hand, two use `self.error()`.
- **`parse_static_block_statements` vs `parse_block_statement` body consolidation**, incl. the
  missing `&& self.current != Token::Eof` guard (not a hang: EOF already errors via the normal
  path; only the message is less precise) and the contingent `HashSet` swap for the O(n·m)
  `lexical_names.contains` scan.
- **`in_non_arrow_function` 8-vs-1 normalization** across eight sites in `expressions.rs`/
  `declarations.rs` outside the seven here (eight of nine decrements sit after a `?`), after
  which the regression test could assert `in_non_arrow_function == 0` unconditionally. Larger and
  riskier; own issue.
- An RAII `Drop` guard (rejected by the issue: fights the borrow checker; closures sidestep it).
- `MAX_PARSE_DEPTH`, lexer, AST types, formatting churn, and rolling `test262-pass.txt`.
