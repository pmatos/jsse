# Plan: issue #375 — TCO-heuristic regression tests: `tests/` vs `test262-extra/`

## 1. Problem restated

jsse has two regression tests that guard its tail-call-optimization (TCO)
eligibility heuristic (`expr_may_contain_tail_call` / the `in_tail_position`
flag machinery in `src/interpreter/eval.rs`) against **false positives** —
cases where the engine wrongly treats a non-tail call as tail-call-eligible
(`tests/tail-position-leaks-into-non-tail-subexpressions.js`,
`tests/tail-call-in-try-block.js`). Both currently live in the plain `tests/`
directory (run by `scripts/run-custom-tests.py`, throw-to-fail, no
frontmatter). PR #372 added a reviewer comment arguing these belong in
`test262-extra/` instead, with test262-style `/*--- ... ---*/` frontmatter,
because AGENTS.md says "any validation that's spec-correct but not in
test262 should have its own tests in `test262-extra/`". A second, local
reviewer disagreed, on the grounds that the existing sibling test
(`tail-call-in-try-block.js`) already set precedent for `tests/`, and that
AGENTS.md's rule is more plausibly aimed at spec-*feature* completeness gaps
than at guards for an engine-internal heuristic's implementation bug. This
plan resolves the ambiguity with a single convention (documented, not just
decided ad hoc) and applies it uniformly to both existing files rather than
moving just one.

## 2. Spec basis

Both tests assert observable behavior required by **ECMA-262, Static
Semantics: `HasCallInTailPosition`** (`spec/spec.html`,
`id="sec-static-semantics-hascallintailposition"`, `oldids
="sec-statement-rules,sec-expression-rules"` — confirmed present in the
vendored spec). This SDO recursively defines which sub-expressions of a
tail-call-eligible context are actually in tail position: e.g. a
`ConditionalExpression`'s taken branch, a `LogicalExpression`'s
short-circuited right operand, and the last element of a comma `Expression`
return `true` through their nested call, while array/object literal
elements, computed keys, `new`/call arguments, and template substitutions
are not part of any `HasCallInTailPosition` production at all (the SDO never
descends into them), so a call there is never in tail position. For the
`try`-block test specifically, the exact grounding rule is the
`TryStatement` productions of this same SDO (`spec/spec.html` lines
~25744–25758): `TryStatement : try Block Catch` returns
`HasCallInTailPosition of Catch` — the `Block` is not consulted at all — and
both `try Block Finally` and `try Block Catch Finally` return
`HasCallInTailPosition of Finally`, again never the `Block`. So a call
anywhere in a `try` Block is categorically excluded from tail position,
independent of what it evaluates to; this is the precise clause the
`tail-call-in-try-block.js` guard exercises. Together with **§15.10.3,
`PrepareForTailCall`** (which discards the caller's frame only when the call
site *is* such a tail position), this is the full spec basis for both files.
jsse implements this dynamically via `expr_may_contain_tail_call` /
`in_tail_position` in `src/interpreter/eval.rs` rather than as a static AST
predicate; both tests exist because that dynamic approximation once
over-matched (treating a non-tail-position call as eligible), which is a
correctness bug against `HasCallInTailPosition`/`PrepareForTailCall`, not a
style nit. This is a test-classification decision, not a behavior change: no
production code is touched, so per the exit criteria of this stage there is
no new syntax/semantics to ground beyond confirming the existing tests are
already spec-grounded (they are — both already cite the relevant behavior in
their descriptive header comments, just not in frontmatter form).

## 3. Files to touch

- `tests/tail-position-leaks-into-non-tail-subexpressions.js` — delete (content moves).
- `tests/tail-call-in-try-block.js` — delete (content moves).
- `test262-extra/tail-position-leaks-into-non-tail-subexpressions.js` — new, test262-style frontmatter + `assert`/`assert.sameValue`/`assert.throws` bodies ported from the `tests/` original.
- `test262-extra/tail-call-in-try-block.js` — new, same treatment.
- `CLAUDE.md` (repo root) — `AGENTS.md` is a symlink to it, so editing `CLAUDE.md` alone updates both. Sharpen the existing bullet under "Testing" ("Any validation that's spec-correct but not in test262 should have its own tests in `test262-extra/` … it should include spec part that is tested and follow the exact same patterns of test262 tests.") with one clause clarifying scope: `test262-extra/` is for regressions whose assertion is a spec-observable value/throw (`assert.sameValue`/`assert.throws`-shaped), including engine-heuristic bugs whose symptom is such a difference (e.g. TCO eligibility) — **not** distinguished by needing `$262` host hooks, since `test262-extra/construct-tail-call-gc-root.js` already calls `$262.gc()` under `run-test262.py`. `tests/` remains for checks that are not naturally test262-shaped: exact host-compatibility diagnostic *message* text (`read-only-assignment-receiver-diagnostic.js`, `read-only-assignment-destructuring-compound.js`, which pin literal TypeError strings, not just the throw), and resource-limit/engine-internals behavior (`recursion-limit-interpreter.js`, `recursion-limit-parser.js`, `function-to-string-large-source.js`'s large-input stress shape). Do not assert this covers every remaining `tests/` file — `strict-mode-reserved-words.js`, `array-join-cyclic.js`, and `basic-expressions.js` are not audited by this change; note them as a follow-up (see §7). No wording changes beyond the one clarifying clause — do not rewrite the surrounding bullets.
- No `docs/adr/` entry: this is a test-placement clarification of an existing documented rule, not a new architectural decision.

## 4. TDD slices

This is a test/documentation reorganization, not a feature; "red/green" here
means "the moved test still fails the same way before the (imaginary) fix and
passes after," but since there is no production bug to fix, the slices are
about proving the move is behavior-preserving.

1. **Slice 1 — port `tail-call-in-try-block.js`.**
   - Write `test262-extra/tail-call-in-try-block.js` with frontmatter:
     `flags: [onlyStrict]`, `features: [tail-call-optimization]`,
     `esid: sec-static-semantics-hascallintailposition`, `description`/`info`
     summarizing the guard and citing the exact grammar production from §2
     (`TryStatement : try Block Catch` → `HasCallInTailPosition of Catch`,
     never `Block`).
   - Convert the two `throw new Error(...)` assertions to
     `assert.sameValue(r, "caught:boom", ...)` and
     `assert.sameValue(count(200000, 0), 200000, ...)`. Keep the per-function
     `"use strict"` pins as-is even under `onlyStrict` (matches the existing
     `construct-tail-call-preserves-ptc.js` convention of pinning strict mode
     locally rather than relying solely on the frontmatter flag).
   - Run it standalone: `uv run python scripts/run-test262.py test262-extra/tail-call-in-try-block.js` — must pass (engine behavior is already correct; this only proves the port is faithful). Note this runner enforces a 120s/512MB limit per test vs. `run-custom-tests.py`'s 10s/no memory cap — irrelevant for this fast test but worth knowing if a future port times out differently.
   - Delete `tests/tail-call-in-try-block.js`.
2. **Slice 2 — port `tail-position-leaks-into-non-tail-subexpressions.js`.**
   - Same treatment: frontmatter with `flags: [onlyStrict]`,
     `features: [tail-call-optimization]`, `esid:
     sec-static-semantics-hascallintailposition`, `info` condensing the
     existing prose comment (the `expr_may_contain_tail_call`
     over-approximation bug from jsse#357/#372) and citing that array/object
     literal elements, computed keys, `new`/call arguments, and template
     substitutions are simply absent from the `HasCallInTailPosition`
     grammar table, so a call there can never be in tail position.
   - Convert each `if (...) throw new Error(...)` block to a matching
     `assert.sameValue` call, one-to-one — no `assert.deepEqual` (it needs
     `includes: [deepEqual.js]`, an unnecessary dependency here); expand
     multi-field checks (e.g. `!Array.isArray(r) || r.length !== 1 || r[0]
     !== "B"`) into 2–3 individual `assert.sameValue`/`assert.sameValue(true,
     ...)` calls instead, still one group per original `if` block. Covers
     array/object literal, computed key, `new` argument, template
     substitution, optional-chain computed key, optional-chain call
     argument, assignment/update target, class computed key, `import()`, and
     the three "genuine tail call must still work" cases at the end.
   - The `import()` sub-case additionally needs `features: [dynamic-import]`
     in the frontmatter (alongside `tail-call-optimization`) and must attach
     `.catch(function () {})` to the returned promise (as the `tests/`
     original implicitly relies on process-level unhandled-rejection
     tolerance that `run-test262.py`'s harness may not share) — verify this
     empirically when running the standalone check; if an unhandled
     rejection fails the test under `run-test262.py`, add the `.catch`.
   - Run standalone: `uv run python scripts/run-test262.py test262-extra/tail-position-leaks-into-non-tail-subexpressions.js` — must pass.
   - Delete `tests/tail-position-leaks-into-non-tail-subexpressions.js`.
3. **Slice 3 — sharpen the CLAUDE.md rule (AGENTS.md symlinks to it).**
   - Add the one clarifying clause described in "Files to touch" under the
     existing `test262-extra/` bullet in the Testing section.
   - No test gate for this slice (it's prose); verify by re-reading the
     bullet and confirming it doesn't contradict the surviving `tests/`
     entries (`recursion-limit-interpreter.js`,
     `recursion-limit-parser.js`, `function-to-string-large-source.js`,
     `array-join-cyclic.js`, `basic-expressions.js`,
     `strict-mode-reserved-words.js`, `read-only-assignment-*.js` — these
     all stay in `tests/` and should still make sense there under the
     sharpened wording).
4. **Slice 4 — full regression sweep.**
   - `uv run python scripts/run-custom-tests.py` (two fewer files, everything else unchanged; must still be 100% pass).
   - `uv run python scripts/run-test262.py test262-extra/` (two more files; must still be 100% pass — this directory has no baseline gating, unlike `test262/`).
   - `cargo test --release` (workspace unit/integration tests untouched by this change, but cheap to confirm).

## 5. Test surface

- No `test262/test/...` directory is implicated — this change touches only
  jsse's own custom-test corpora (`tests/`, `test262-extra/`), not the
  upstream submodule, and involves no engine code.
- Gate for this change: `uv run python scripts/run-test262.py test262-extra/`
  (covers the two moved files) and `uv run python scripts/run-custom-tests.py`
  (covers everything remaining in `tests/`, proving nothing else regressed
  when the two files are removed).
- `test262-pass.txt` is not touched and not expected to move — this PR adds
  no engine behavior, so the baseline diff should be empty.

## 6. Regression risk

- Zero engine-code risk: no `src/` files change, so none of `eval_expr`,
  `exec_statement`, `property.rs`, GC rooting/`gc_safepoint()`, the
  `ObjectKind` matches, or the bytecode fast path are touched.
- The only behavioral risk is a faithfulness bug introduced while porting
  `throw`-based assertions to `assert.sameValue`/`assert.throws` calls (e.g.
  accidentally weakening a check, or dropping one of the ~10 sub-cases in
  the leaks-into-non-tail-subexpressions test during translation). Mitigated
  by running each ported file standalone before deleting its `tests/`
  original (slices 1–2), and by keeping a 1:1 mapping from each `if (...)
  throw` block to one `assert.*` call rather than consolidating or
  rewriting the test logic.
- `esid`/`features` frontmatter risk is low: confirmed
  `scripts/run-test262.py` parses `features:` (`FEATURES_RE`) but has no
  feature-based skip/exclude list, so adding `tail-call-optimization` or
  `dynamic-import` to frontmatter only affects reporting, not whether the
  test runs — the existing `construct-tail-call-*.js` files already prove
  this (they run and pass today). The `esid` value
  (`sec-static-semantics-hascallintailposition`) was confirmed present in
  `spec/spec.html` directly (§2), so no guessing is needed at implementation
  time.

## 6a. PR title and decision record

- Proposed PR title (squash subject, commitlint-safe):
  `test(tco): move tail-position regression guards to test262-extra`.
- Per the operating contract, the PR body (or a `gh issue comment 375` if
  no PR text field is more suitable) must record the decision made here and
  why, since two AI reviewers disagreed on PR #372: state that both guards
  move to `test262-extra/` because they assert spec-observable behavior
  (`HasCallInTailPosition`/`PrepareForTailCall`), matching the three
  existing TCO-related files already there
  (`construct-tail-call-preserves-ptc.js`,
  `construct-return-tail-call-nonobject.js`,
  `construct-tail-call-gc-root.js`); note Codex's position (FIX/HIGH) and
  the local Opus reviewer's position (REJECT/MEDIUM, precedent-based) were
  both considered, and this resolves the ambiguity by sharpening the
  underlying CLAUDE.md rule so future placement decisions don't require
  re-litigating it per-PR.

## 7. Out of scope

- Do not re-litigate or move any of the other `tests/` files
  (`recursion-limit-interpreter.js`, `recursion-limit-parser.js`,
  `function-to-string-large-source.js`, `array-join-cyclic.js`,
  `basic-expressions.js`, `strict-mode-reserved-words.js`,
  `read-only-assignment-destructuring-compound.js`,
  `read-only-assignment-receiver-diagnostic.js`) — none were named in the
  issue, and the sharpened wording in slice 3 is meant to justify their
  continued placement, not trigger a fresh audit of them.
- Do not touch `test262-extra/construct-tail-call-preserves-ptc.js`,
  `test262-extra/construct-return-tail-call-nonobject.js`, or
  `test262-extra/construct-tail-call-gc-root.js` — they already follow the
  target convention and are unrelated to this issue's two false-positive
  heuristic guards.
- Do not attempt to fix, refactor, or add features to
  `expr_may_contain_tail_call`/`in_tail_position` — the heuristic is already
  correct per PR #372 and #376; this issue is pure test/documentation
  placement.
- Do not add a `docs/adr/` entry — the existing CLAUDE.md (AGENTS.md
  symlink) bullet already documents the convention; this is a one-clause
  sharpening, not a new architectural decision worth its own ADR.
- Do not run or update `--update-baseline` — no engine behavior changes, so
  `test262-pass.txt` is untouched by this PR.

**Follow-up (not this PR):** audit the remaining `tests/` files
(`strict-mode-reserved-words.js`, `array-join-cyclic.js`,
`basic-expressions.js`) against the sharpened CLAUDE.md rule — they were not
opened closely enough during this planning pass to confirm they clearly fit
`tests/` under the new wording (in particular `strict-mode-reserved-words.js`
and `array-join-cyclic.js` assert plain spec-observable behavior with no
host-diagnostic or resource-limit angle, so they may also be candidates for
`test262-extra/`). File a separate issue if the sharpened rule suggests they
should move too, rather than expanding this PR's scope.

**Commit hygiene:** the working tree has an unrelated dirty `test262`
submodule pointer (pre-existing, not created by this work). Stage and commit
only the files listed in §3 — never `git add -A`/`git add .` — so the
submodule pointer noise doesn't get swept into this PR's commits.
