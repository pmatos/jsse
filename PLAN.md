# Plan: issue #873 — compile labeled statements with labeled break/continue

## 1. Problem restated

The bytecode compiler (`src/interpreter/bytecode/compiler.rs`) compiles `While`
and `For` loops but has no case for `Statement::Labeled`, `Statement::Break`,
or `Statement::Continue`: all three fall through to the catch-all arm in
`compile_statement` and bail the *entire enclosing function body* back to the
tree-walker. Mandreel's C→JS translation emits every loop as a labeled
`while(true){...}` using labeled `break`/`continue` for control flow (649
occurrences), so in practice this means `While`/`For` compilation — already
implemented and measured well above break-even (#539) — never actually fires
on mandreel's hottest functions. A `perf-counters` audit on `main` (`7f2abfc4`)
shows ~99% of the run's remaining tree-walker AST work sits in bodies whose
compile bail reason is `statement:Labeled`, led by `sortMinDown`/`sortMaxDown`
at 80.7% combined. This plan adds compiler support for `break`/`continue`
(labeled and unlabeled) bound to `while`/`for` loops, which is the minimal
change that unblocks every bail in the audit's top rows.

## 2. Spec basis

- **§14.7.3 The `while` Statement** (`sec-while-statement`) and its Runtime
  Semantics: WhileLoopEvaluation (`sec-runtime-semantics-whileloopevaluation`)
  — defines that each iteration re-evaluates the test, and that the loop's V
  accumulator is updated only on a non-empty body completion.
- **§14.7.4 The `for` Statement** (`sec-for-statement`) and its Runtime
  Semantics: ForBodyEvaluation (`sec-runtime-semantics-forbodyevaluation`,
  under `sec-runtime-semantics-forloopevaluation`) — defines that `continue`
  must still run the increment expression before the next test, i.e. the
  per-iteration control point for `continue` sits *after* the body and
  *before* the update, not at the top of the loop.
- **§14.7.1.1 LoopContinues** (`sec-loopcontinues`) — the shared predicate
  both loop forms use to decide whether an abrupt `break`/`continue`
  completion keeps the loop going (label-set membership) or propagates.
- **§14.8 The `continue` Statement** (`sec-continue-statement`) and its
  Runtime Semantics (`sec-continue-statement-runtime-semantics-evaluation`) —
  plain `continue;` produces `Completion Record { [[Type]]: continue,
  [[Value]]: empty, [[Target]]: empty }`; `continue Label;` sets `[[Target]]`
  to the label. Its early-error clause
  (`sec-continue-statement-static-semantics-containsundefinedcontinuetarget`)
  is what makes `continue Label` a syntax error unless `Label` labels an
  enclosing `IterationStatement` — the parser already enforces this, so by
  the time the compiler sees `Statement::Continue(Some(name))`, `name` is
  guaranteed to label an enclosing loop.
- **§14.9 The `break` Statement** (`sec-break-statement`) and its Runtime
  Semantics (`sec-break-statement-runtime-semantics-evaluation`) — analogous
  to continue, `[[Type]]: break`, and (unlike continue) legal unlabeled
  inside a `switch` too, which is irrelevant here since `Switch` isn't
  compiled.
- **§14.13 Labelled Statements** (`sec-labelled-statements`), its early errors
  (`sec-labelled-statements-static-semantics-early-errors`, duplicate-label
  rule for stacked labels), and Runtime Semantics: LabelledEvaluation
  (`sec-runtime-semantics-labelledevaluation`, §14.13.4) — a `break` whose
  target matches the label converts that `break` completion to
  `Completion::Normal`; stacked labels (`a: b: while (...) {}`) each wrap the
  same iteration statement and are tried against the label set independently.
  Existing engine-side precedent for this is `src/interpreter/exec.rs:1130`
  (`Statement::Labeled` arm) and `handle_loop_body_completion`
  (`src/interpreter/exec.rs:3014`), which already implement this correctly
  for the tree-walker; the compiler changes mirror that control-flow shape,
  not the AST-tree-walking mechanism.

No new JavaScript syntax or semantics are introduced — this slice makes the
existing bytecode compiler accept constructs the parser and tree-walker
already support, lowering them to equivalent bytecode.

## 3. Files to touch

- `src/interpreter/bytecode/compiler.rs` — the only production-code file:
  - Add a `LoopFrame` struct and a `loop_frames: Vec<LoopFrame>` field on
    `Compiler`.
  - Factor `Statement::While`/`Statement::For` compilation out of
    `compile_statement`'s match arms into `compile_while`/`compile_for`
    methods parameterized by a label list, so both the bare
    (`Statement::While(w) => self.compile_while(w, Vec::new())`) and labeled
    paths share one lowering.
  - Add a `Statement::Labeled` arm that peels consecutive nested
    `Statement::Labeled` wrappers (for `a: b: while (...) {}`) down to the
    wrapped statement, and dispatches to `compile_while`/`compile_for` with
    the collected label names if that statement is a loop, else bails with
    `CompileError::Unsupported("statement:Labeled")` (unchanged reason
    string, so `BAIL` counters keep meaning).
  - `LoopFrame` carries `labels: Vec<String>`, `continue_target:
    Option<usize>` (`Some(loop_start)` for `while`, known up front;
    `None` for `for`, filled in once the body has compiled and the update's
    position is known), and `break_sites: Vec<usize>` (always deferred —
    the position after the loop is never known while compiling the body).
    `while`'s `continue` resolves immediately: once the frame's
    `continue_target` is `Some(loop_start)`, `Statement::Continue` emits a
    *direct* backward jump via the existing `emit_jump_to(Op::Jump,
    loop_start)` — no patch site needed, since the target is already behind
    the jump. `for`'s `continue` can't do this (the target isn't known until
    after the body compiles), so it instead records a placeholder-jump patch
    site (`emit_jump(Op::Jump)`) into a *second*, `for`-only list on the
    frame, patched with the existing `patch_jump` (which already patches to
    "current end of code") once `compile_for` reaches the point between body
    and update. `break` always uses the placeholder-plus-`patch_jump` form
    in both loop kinds, since the after-the-loop position is never known
    while the body compiles. No new patch-target-computation helper is
    needed; the existing `patch_jump` (patch-to-current-end) and
    `emit_jump_to` (emit-to-known-target) already cover every case here.
  - Add `Statement::Break`/`Statement::Continue` arms that resolve the target
    frame (innermost frame for unlabeled, label-set search for labeled).
  - Update `statement_kind`'s existing exhaustive match: no change needed —
    `Statement::Break`/`Continue`/`Labeled` already have entries; they just
    stop being reachable as compile-bail causes for the cases now handled.
- `src/interpreter/bytecode/tests.rs` — see TDD slices below; one existing
  test (`loop_with_break_falls_back_to_tree_walker`, line ~1714) asserts the
  *old* behavior and must be replaced, not left in place, since this PR makes
  that construct compile.
- `docs/adr/` — add `docs/adr/<timestamp>-bytecode-labeled-loop-lowering.md`
  (timestamp set at authoring time per `docs/adr/README.md`'s `YYYY-MM-DD-HHMM`
  convention) documenting: the per-loop `LoopFrame` design; why `while`'s
  `continue` resolves immediately as a direct backward `emit_jump_to` (its
  target, `loop_start`, is known before the body compiles) while `for`'s
  `continue` must defer through a placeholder-jump-plus-`patch_jump` (its
  target — the position between body and update — isn't known until the
  body has finished compiling); and why stacked labels share one frame
  instead of nesting frames (mirrors `sec-runtime-semantics-labelledevaluation`'s
  label-set semantics, and matches the tree-walker's existing behavior at
  `src/interpreter/exec.rs:1130`).
- No changes to `src/parser/`, `src/ast.rs`, `src/interpreter/exec.rs`,
  `src/interpreter/eval.rs`, or `src/interpreter/bytecode/vm.rs` /
  `src/interpreter/bytecode/op.rs`: the VM's `Op::Jump` already exists and
  already safepoints on any negative-offset jump
  (`src/interpreter/bytecode/vm.rs:708-713`), independent of which compiler
  construct emitted it, so lowering `continue` to a backward `Op::Jump` gets
  GC-safepoint parity for free and needs no VM changes.
- `test262-extra/` — one new file (see §5); `benchmarks/scripts/bench_opmix.js`
  — one new labeled-loop benchmark variant (see §5), manual validation
  evidence for the PR description, not a gated test.

## 4. TDD slices

Each slice is a red test in `src/interpreter/bytecode/tests.rs` (using the
existing `assert_parity_number`/`eval_with_mode` helpers already in that file)
followed by the minimal compiler change that turns it green.

1. **Unlabeled `break` in a bare `while(true)` compiles.**
   Red: a new test asserting
   `"var __r = (function(){ var i = 0; while (true) { i++; if (i > 2) break; } return i; })();"`
   takes the bytecode path (`bc_count >= 1`) and returns `3.0`. This replaces
   `loop_with_break_falls_back_to_tree_walker`'s assumption — rename it (e.g.
   `loop_with_break_takes_bytecode_path`) and flip its assertion from
   `count == 0` to `count >= 1`, keeping its `.0` value check.
   Green: `LoopFrame`/`loop_frames` field, `compile_while` pushes an unlabeled
   frame, `Statement::Break(None)` resolves to the innermost frame and emits a
   forward `Op::Jump` collected into `break_sites`, patched (alongside the
   existing `exit` patch) to the position right after the loop.

2. **Unlabeled `continue` in `while` re-tests the condition.**
   Red: `"var __r = (function(){ var i = 0, n = 0; while (i < 5) { i++; if (i % 2 === 0) continue; n += i; } return n; })();"`
   expect `9.0` (1+3+5), bytecode path taken.
   Green: `Statement::Continue(None)` resolves the innermost frame; since
   `compile_while` set that frame's `continue_target` to `Some(loop_start)`
   before compiling the body, `Continue` emits a direct backward jump via
   `emit_jump_to(Op::Jump, loop_start)` — no deferred patch needed.

3. **Unlabeled `break`/`continue` in `for` (continue must still run the
   update).**
   Red: `"var __r = (function(){ var n = 0; for (var i = 0; i < 10; i++) { if (i === 2) continue; if (i === 5) break; n += i; } return n; })();"`
   — iterations add `0, 1`, skip `2`, add `3, 4`, then break at `5`: expect
   `8.0`. Assert this with `assert_parity_number`, which additionally
   cross-checks the bytecode result against the tree-walker's own
   (independently-computed) value for the same source.
   Green: `compile_for` pushes a frame with `continue_target: None`; each
   `continue` compiled while the frame has no target yet is recorded as a
   placeholder-jump patch site in a `for`-only list, patched via the
   existing `patch_jump` once `compile_for` reaches the point between body
   and update (where `self.code.len()` *is* that target); break sites patch
   to after the loop, same as `while`.

4. **Labeled loop with matching labeled `break`/`continue` (the mandreel
   shape).**
   Red: `"var __r = (function(){ var n = 0; outer: while (true) { n++; if (n === 2) continue outer; if (n === 4) break outer; } return n; })();"`
   expect `4.0`.
   Green: `Statement::Labeled` arm peels the single label, calls
   `compile_while(w, vec!["outer".into()])`; `Break`/`Continue` with
   `Some("outer")` search `loop_frames` by label membership (not just
   innermost) and resolve to the same frame an unlabeled break/continue
   there would use.

5. **Labeled `break`/`continue` targeting an *outer* loop from inside a
   nested loop.**
   Red: `"var __r = (function(){ var n = 0; outer: for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if (j === 1) continue outer; n++; } } return n; })();"`
   expect `3.0` (inner loop runs once per outer iteration before `continue
   outer` skips the rest).
   Green: label search must walk `loop_frames` outward (not just the
   innermost frame), confirming labeled break/continue correctly bypasses an
   intervening unlabeled loop.

6. **Stacked labels on one loop (`a: b: while`).**
   Red: `"var __r = (function(){ var n = 0; a: b: while (n < 5) { n++; if (n === 3) break a; } return n; })();"` expect `3.0`, and a second
   case using `break b;` on the same source shape to confirm either label
   exits the same (single) loop.
   Green: the label-peeling loop in the `Statement::Labeled` arm collects
   both names into one `LoopFrame.labels`, not two nested frames.

7. **Labeled non-loop statement still bails (unchanged, regression-guard).**
   Red/stays-red-by-design: a test asserting
   `"var __r = (function(){ outer: { break outer; } return 1; })();"`
   still falls back to the tree-walker (`bc_count == 0`) and still returns
   `1.0` — this is intentionally out of scope for this slice (see §7).
   No production change should make this pass; the test documents the
   boundary and must keep asserting `count == 0`.

8. **Script-goal completion value (`CompileGoal::Script`) through
   `break`/`continue`.** `While`/`For`'s `reset_script_completion`/
   `SetCompletion` bookkeeping is untouched by this PR, but it's only
   exercised by the existing If/While/For tests without break/continue in
   the mix — add cases (in the style of
   `script_statement_list_completion_matches_tree_walker`, line ~1606,
   via `assert_script_completion_number`/`assert_script_completion_undefined`)
   for:
   - `"while (true) { 1; break; }"` → `1.0`
   - `"1; while (true) { break; }"` → undefined (the `1;` completion is
     overwritten by `reset_script_completion` at loop entry, and `break`
     carries no value forward)
   - `"for (var i = 0; i < 3; i++) { i; continue; }"` → `2.0`
   - `"x: while (true) { 5; while (true) { break x; } }"` → undefined, per
     WhileLoopEvaluation's `UpdateEmpty(stmtResult, V)` with the *inner*
     loop's `V` (which starts `undefined` and is never updated, since the
     inner loop's only statement is `break x`, not an expression statement)
     flowing out through the label. This is the trickiest of the four and
     the one most likely to expose a pre-existing tree-walker gap per the
     caveat below — write it, see what both paths actually produce, and
     treat a tree-walker/spec mismatch as a tree-walker bug to flag in the
     PR, not a reason to match the bytecode lowering to it.
   Derive each expected value from §14.7.1.1 LoopContinues / WhileLoopEvaluation's
   `UpdateEmpty(stmtResult, V)` step, not from running the tree-walker first;
   `assert_script_completion_*` already cross-checks both paths and fails
   loudly (`"tree-walker completion for {source}"`) if the tree-walker
   itself disagrees with the spec derivation, which would be a tree-walker
   bug to flag in the PR rather than a reason to bend the bytecode lowering.
   Green: no new production code expected — this slice exists to prove the
   existing `SetCompletion` bookkeeping composes correctly with the new
   `Break`/`Continue` arms (which emit no `SetCompletion` of their own, per
   §3); if it doesn't compose, the fix belongs in `compile_while`/
   `compile_for`'s existing `reset_script_completion` call, not in a new
   code path.

## 5. Test surface

- **test262, targeted re-runs, in *both* modes** (run after each slice, not
  just at the end — default mode is the tree-walker regression guard,
  `--bytecode` is the only test262-level signal that the new lowering itself
  didn't break anything, since `bytecode_enabled` defaults to `false` and
  `--bytecode` is what flips it for the runner):
  - `test262/test/language/statements/while/` (38 files)
  - `test262/test/language/statements/for/` (385 files)
  - `test262/test/language/statements/break/` (20 files)
  - `test262/test/language/statements/continue/` (24 files)
  - `test262/test/language/statements/labeled/` (24 files)
  - Commands: `uv run python scripts/run-test262.py test262/test/language/statements/<dir>/`
    and `uv run python scripts/run-test262.py --bytecode test262/test/language/statements/<dir>/`
    for each directory, then the full default suite
    (`uv run python scripts/run-test262.py`) and the full `--bytecode` suite
    (`uv run python scripts/run-test262.py --bytecode`) before opening the
    PR. Both compare against `origin/main:test262-pass.txt` (no
    `--update-baseline`); `--bytecode`'s pass/fail set is not expected to
    match the default run's exactly (many files in these directories exercise
    `throw`/`try`/`typeof`/generators/etc. that still bail the whole body to
    the tree-walker even under `--bytecode`), but neither run may *regress*
    against the baseline.
- **`cargo test --release`** covers the new `src/interpreter/bytecode/tests.rs`
  cases directly (fast feedback loop during the TDD slices).
- **`test262-extra/`, required:** per `AGENTS.md`, `test262-extra/` covers
  "regressions for engine-internal heuristics when the failure changes an
  observable ECMAScript value or throw" — a label-frame resolution bug (wrong
  frame picked, `for`'s deferred continue-patch landing in the wrong place,
  etc.) is exactly that: it changes an observable value only under
  `--bytecode`, and `ci.yml` already runs `test262-extra/` under both normal
  and `--bytecode` (plus `JSSE_GC_STRESS=7`) on every PR, so a new file here
  is the only *automated*, every-PR `--bytecode` coverage this change gets —
  the in-crate `tests.rs` parity slices (§4) are excellent for TDD but are
  not part of that CI gate. Add one file, e.g.
  `test262-extra/bytecode-labeled-loop-break-continue.js`, in test262
  frontmatter style, citing `esid: sec-runtime-semantics-labelledevaluation`
  (primary) and `sec-runtime-semantics-forbodyevaluation` (for the
  `for`-continue case) in its `info`. Constraints to keep it exercising the
  bytecode path specifically:
  - Every function body under test must stay within what the compiler
    already accepts (`var`, `if`, `while`, `for`, numeric/string literals,
    arithmetic/comparison, plain calls) — no `throw`/`try`/`typeof`/lexical
    declarations, or the whole body bails to the tree-walker and the file
    stops testing anything bytecode-specific.
  - Call the functions and `assert.sameValue` from top level, outside any
    function body, so the assertions themselves don't need to compile.
  - Cases: labeled `while (true)` with matching `break`/`continue` (the
    mandreel shape from slice 4), `continue outer` reaching past an inner
    loop (slice 5), `for`'s `continue` still running the update (slice 3),
    and stacked labels (slice 6) — i.e. the same shapes as the `tests.rs`
    slices, re-expressed as a standalone script so CI's `--bytecode` gate
    exercises them without needing `interp.bytecode_enabled` set by hand.
  - This file does not assert *which* engine path ran — unlike `tests.rs`,
    test262-extra files run under both normal and `--bytecode` CI jobs and
    must pass either way; the value equality is the test, not the path
    taken.
- **GC stress:** `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --timeout 300` and the same for `--bytecode`, per repo convention — this
  change adds backward jumps (the `continue`-in-`while` case, and the
  frame-close backward patch) that cross the VM's existing
  `safepoint_with_empty_stacks` call for negative-offset `Op::Jump`
  (`vm.rs:708-713`); stress mode is the right lever to catch a missed root if
  the label-frame bookkeeping somehow left a live value off the operand stack
  at that point. Expect no stress-only failures since no new rooting is
  introduced (`current_stack == 0` at every `break`/`continue` site, asserted
  per slice 1-3's green step).
- **Benchmarks (validation, not gating):** extend
  `benchmarks/scripts/bench_opmix.js` with a labeled `while(true)` +
  `break`/`continue` variant alongside the existing `arith`/`elem`/`leaf`/
  `mixed` functions, per the issue's suggested validation step; expect it to
  land in the same `elem`/`mixed` 19-29%-over-tree-walker band. Then re-run
  the mandreel per-phase driver
  (`scripts/gen-mandreel-phases.py <mandreel.js> -o driver.js`) and the #54
  reference-box retest to confirm the `statement:Labeled` bail disappears
  from `sortMinDown`/`sortMaxDown`'s `BAIL` rows and the wall-clock delta
  lands near the issue's 22-27s estimate. This is manual validation evidence
  for the PR description, not an automated gate.

## 6. Regression risk

- **`test262-pass.txt` baseline:** this change only *adds* compile
  eligibility (fewer bails, more chunks run on the VM); it cannot regress a
  test that currently passes via the tree-walker fallback unless the new
  bytecode lowering produces a different observable value than the
  tree-walker for some break/continue/label shape the TDD slices above don't
  cover. The parity-style tests (`assert_parity_number`, which runs *both*
  paths and diffs the result) are the direct mitigation; running the full
  `language/statements/{while,for,break,continue,labeled}` directories plus
  the full suite before opening the PR is the test262-level mitigation. Per
  repo convention this PR does **not** run `--update-baseline` — that stays a
  `main`-branch operation.
- **Shared machinery leaned on:**
  - The bytecode VM's `Op::Jump` dispatch and its sign-based safepoint
    (`vm.rs:708-713`) — reused as-is, not modified; risk is low because it's
    generic over jump *offset*, not origin construct.
  - `patch_jump` and `emit_jump_to` themselves are not modified, only reused
    by the new `Break`/`Continue` arms — the existing TDD slices for
    If/While/For already in `tests.rs` (lines ~1235-1719) are the regression
    guard for those two helpers and should be re-run first (`cargo test
    --release`), before any new test is added, to confirm the `compile_while`/
    `compile_for` extraction (factoring the existing match arms into methods,
    per §3) didn't change their behavior.
  - `current_stack`/`current_refs` invariants (`debug_assert_eq!` calls
    already present around loop bodies) — `Break`/`Continue` compile must
    assert `current_stack == 0` before emitting, matching the "statements
    are net-zero on the stack" invariant documented in the issue; a violation
    here would panic under `cargo build --profile release-checked` /
    `cargo test` (debug), which is exactly the intended tripwire, not a
    silent corruption risk.
  - `gc_bytecode_roots` LIFO discipline (`gc.rs`'s root-stack assertions) —
    `break`/`continue` lower to plain `Op::Jump`, which touches no roots, so
    this is unaffected; the GC-stress run in §5 is the empirical check.
  - The tree-walker (`exec.rs`) itself is untouched — it remains the
    semantic reference and the fallback path for anything still unsupported
    (labeled non-loop statements, `DoWhile`, `ForIn`/`ForOf`, lexical
    declarations in loop heads, etc.), so no tree-walker regression risk.
  - `perf_counters.rs`'s `BAIL`/`bail_by_name` tables — no code change there;
    their output simply shifts (fewer `statement:Labeled` bails reported)
    once this lands, which is the intended, measurable effect for the
    before/after validation in §5.

## 7. Out of scope

- Labels on non-loop statements (e.g. `block: { break block; }`) — legal JS,
  currently bails, stays bailing after this PR (slice 7 pins this). Revisit
  only if a future audit shows it on a hot path; the issue's own "Suggested
  design" section explicitly defers it.
- `DoWhile`, `ForIn`, `ForOf` loop compilation — none of these are compiled
  today (all hit the `compile_statement` catch-all already), and labeling
  them is a separate, larger slice (each needs its own loop-body compiler
  first). Not touched here.
- `Statement::Switch` — `break` inside an unlabeled `switch` with no
  enclosing loop is legal and would need a `switch`-targeted frame kind; out
  of scope since `Switch` isn't compiled at all yet.
- The `u16` constant-pool-overflow precursor the issue flags as "small
  precursor or follow-up" (`compiler.rs:60-67`, widening to `u32`) — separate
  concern with its own VM-side decode changes; not bundled into this PR.
- Any refactor of `exec.rs`'s existing (correct, already-tested)
  `exec_labeled_loop`/`handle_loop_body_completion` tree-walker machinery —
  read for spec parity only, not modified.
- Rolling `test262-pass.txt` forward (`--update-baseline`) — a `main`-branch
  operation per repo convention, not part of this feature branch.
- Re-fitting the VM per-op cost / IC-depth model mentioned in the issue's
  "Expected payoff" as the *next* lever after this lands — explicitly a
  follow-up investigation, not this PR.
