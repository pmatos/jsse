# Bytecode compilation of labeled while/for loops with break/continue

Issue #873: a `perf-counters` audit of the mandreel benchmark showed ~99% of
the run's remaining tree-walker work sitting in bodies whose bytecode
compile bail reason is `statement:Labeled` -- led by `sortMinDown`/
`sortMaxDown` at 80.7% combined. Mandreel's C-to-JS translation emits every
loop as a labeled `while(true){...}` using labeled `break`/`continue` (649
occurrences in mandreel.js), so `compile_statement`'s existing `While`/`For`
support -- already measured well above the per-chunk break-even in #539 --
never actually fired on the hot path: `Statement::Labeled`, `Statement::Break`,
and `Statement::Continue` all hit the catch-all arm and bailed the whole
enclosing function body back to the tree-walker.

## Decision

**A per-loop `LoopFrame` on `Compiler` tracks enough state to resolve
`break`/`continue` without re-walking the AST.** `Statement::While`/`For`
compilation is factored out of `compile_statement`'s match arms into
`compile_while`/`compile_for`, now parameterized by a label list so the bare
form (`Vec::new()`) and the labeled form share one lowering. A `LoopFrame`
holds `labels: Vec<String>`, `continue_target: Option<usize>`,
`continue_sites: Vec<usize>` (deferred continue patch sites), and
`break_sites: Vec<usize>` (deferred break patch sites). `Compiler::loop_frames`
is a stack, innermost last; `break`/`continue` resolve the unlabeled form
against the last frame, and the labeled form via `resolve_loop_frame`
searching outward (`rposition`) for a frame whose `labels` contains the
target name -- which is what lets `continue outer`/`break outer` reach past
an intervening unlabeled inner loop, not just the directly-enclosing one.

**`while`'s continue target is known before its body compiles; `for`'s
isn't.** `compile_while` pushes its frame with `continue_target:
Some(loop_start)` up front, so every `continue` against that frame lowers
immediately to a direct backward `emit_jump_to(Op::Jump, loop_start)` --
`continue_sites` stays empty for a `while` frame by construction (asserted
on frame pop). `compile_for` can't do this: per ForBodyEvaluation
(`sec-runtime-semantics-forbodyevaluation`), `continue` must still run the
update expression before the next test, and the update's position isn't
known until the body has finished compiling. So `compile_for` pushes
`continue_target: None`; any `continue` compiled against that frame instead
records a placeholder `emit_jump` site in `continue_sites`, which
`compile_for` patches (via the existing `patch_jump`, which patches to
"current end of code") immediately after the body and before emitting the
update. `break` always defers through `break_sites` in both loop kinds,
since the post-loop position is never known while the body compiles.

**Stacked labels on one loop share a single frame.** `Statement::Labeled`
peels consecutive nested `Labeled` wrappers (`a: b: while (...) {}`) into one
`Vec<String>` before dispatching to `compile_while`/`compile_for`, rather
than nesting one frame per label. This mirrors LabelledEvaluation's
label-set semantics (`sec-runtime-semantics-labelledevaluation`) -- a
label set, tried independently against the same iteration statement -- and
matches the tree-walker's existing handling of the same construct
(`exec.rs`'s `Statement::Labeled` arm). `break a;` and `break b;` on `a: b:
while (...)` must exit the same loop, not two different ones.

**Labels on non-loop statements stay unsupported.** `Statement::Labeled`
dispatches to `compile_while`/`compile_for` only when the peeled statement is
a `While` or `For`; anything else (e.g. `outer: { break outer; } `) bails
with the same `CompileError::Unsupported("statement:Labeled")` string the
catch-all used before this change, so `BAIL` counters keep their existing
meaning. Every bail in the issue's audit is a labeled `while(true)`, so this
is the minimal change that unblocks the hot path; `DoWhile`/`ForIn`/`ForOf`
aren't compiled at all yet (separate, larger slices), and `Switch`'s
unlabeled `break` is out of scope since `Switch` isn't compiled either.

`break`/`continue` lower to a plain `Op::Jump`, touching no new rooting
(`gc_bytecode_roots` is unaffected) and preserving the existing
"statements are net-zero on the operand stack" invariant -- both compile
methods assert `current_stack == 0`/`current_refs == 0` immediately before
emitting the jump, matching the same assertions already present around the
loop back-edge.

## What's confirmed fixed

All eight TDD slices in `src/interpreter/bytecode/tests.rs` pass, each using
`assert_parity_number` (cross-checks the bytecode result against the
tree-walker's own, independently-computed result for the same source) or
`assert_script_completion_*` (same cross-check, for `CompileGoal::Script`'s
completion-value bookkeeping): unlabeled `break`/`continue` in `while` and
`for` (continue still runs the `for` update), the mandreel labeled-loop
shape, a labeled `continue` reaching past an intervening unlabeled inner
loop, stacked labels where either label exits the same loop, and four
script-completion cases derived from WhileLoopEvaluation/ForBodyEvaluation's
`UpdateEmpty(stmtResult, V)` step (including a labeled `break` out of a
nested `while` carrying the *inner* loop's reset-to-undefined `V`, not the
outer body's prior value, out as the final completion). The regression-guard
test for a labeled non-loop statement still bails to the tree-walker, as
designed.

## What's deliberately left alone

- **Labels on non-loop statements** (`block: { break block; }`) -- legal JS,
  currently bails, stays bailing. Revisit only if a future audit shows it on
  a hot path; the issue's own "Suggested design" section explicitly defers
  it.
- **`DoWhile`, `ForIn`, `ForOf` loop compilation** -- none of these are
  compiled today; each would need its own loop-body compiler before a label
  could attach to it. Not touched here.
- **`Statement::Switch`** -- `break` inside an unlabeled `switch` with no
  enclosing loop is legal and would need a `switch`-targeted frame kind; out
  of scope since `Switch` isn't compiled at all yet.
- **The `u16` constant-pool-overflow precursor** the issue flags as a
  possible follow-up (`compiler.rs`'s `add_constant`, widening to `u32`) --
  separate concern with its own VM-side decode changes, not bundled here.
