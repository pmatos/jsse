# Plan: issue #669 — yield-free `try`/`with` containing `break`/`continue` falls through in state-machine bodies

## 1. Problem restated

The generator transform (`src/interpreter/generator_transform.rs`) emits a
yield-free statement verbatim into a state body and lets the tree-walker run
it. When that statement is a `try`/`with` (or a nested loop/switch) whose
`break`/`continue` escapes it, the tree-walker returns `Completion::Break` /
`Completion::Continue` from the state body. `stmt_has_break_or_continue` does
not recurse into `Try`/`With`/nested loops, so nothing lowers the jump, and the
three state drivers (sync generator, async generator, async function) never
inspect a raw `Break`/`Continue`. They fall through to the state's own
terminator (`Goto(next_case)`, next statement, ...), so the jump is lost.

Reproduced against a release build of this branch (`node` agrees with the spec
in every row):

| shape | jsse | node |
|---|---|---|
| issue example: `case 1: try { break; } finally { l.push('f'); }` in a generator switch | `["f,two"]` | `["f"]` |
| `for(...) { try { if (i==2) break; } finally { log } yield i; }` | runs all 5 iterations | stops at `i==2` |
| same with `continue` | `continue` ignored | correct |
| `case 1: with (o) { l.push(k); break; }` | `1,two` | `1` |
| same switch shape in `async function`, `async function*` | fall-through | correct |

The finalizer already runs (natively, inside the tree-walker) — only the jump
is lost. That is the key fact behind the design below.

## 2. Spec basis

All ids are in `spec/spec.html` (ECMA-262 at the pinned submodule commit):

- `sec-runtime-semantics-caseblockevaluation` (CaseBlockEvaluation): once a
  clause is selected, later clauses run "only until an abrupt completion"; a
  `break`/`continue` completion ends the CaseBlock.
- `sec-try-statement-runtime-semantics-evaluation` (TryStatement Evaluation):
  `Block Finally` — the finalizer *runs*, and if F is normal "set F to B", so
  the original abrupt (break/continue) completion is what the statement
  returns. This is the normative reason the finalizer must run **and** the jump
  must survive. When F is itself abrupt (`finally { continue outer; }`), F
  replaces B.
- `sec-break-statement-runtime-semantics-evaluation` and
  `sec-continue-statement-runtime-semantics-evaluation`: produce
  `break`/`continue` completion records carrying a target label (empty for
  unlabelled).
- `sec-runtime-semantics-labelledevaluation` (LabelledEvaluation): a labelled
  statement consumes a `break` completion whose target is one of its labels;
  `sec-loopcontinues` (LoopContinues): an iteration statement consumes a
  `continue` completion whose target is empty or in its label set.
- `sec-with-statement-runtime-semantics-evaluation`: the body's abrupt
  completion is returned unchanged (after the object environment is popped).
- `sec-updateempty`: abrupt completions propagate through
  Block/Try/With/If with `UpdateEmpty`, never being converted to normal.

No JavaScript syntax changes; only the transform/driver's conformance to these
completion semantics.

## 3. Design decision

The issue text suggests lowering `try`/`with` into states with try-depth-aware
`LoopControl` targets and `with_scopes`. **Not chosen**, for these reasons:

1. A yield-free `try`/`with` has no suspension point. Lowering it buys nothing
   and adds risk: catch-parameter scoping, finalizer-overrides-completion
   (`F` replacing `B`), and completion-value plumbing would all have to be
   re-implemented in states, when the tree-walker already gets them right.
2. Lowering makes the jump a `Goto`/`LoopControl` *out of a transformed try*.
   In the sync/async **generator** drivers that skips finalizers (see §8 —
   this is a separate pre-existing bug reproduced with a yielding `try`). A
   detection-only or lowering-only prototype therefore trades "finalizer runs,
   falls through" for "no fall-through, finalizer skipped", which is exactly
   the failure the issue reports for its prototype.

**Chosen — inline-jump capture.** Keep the yield-free statement inline, and
teach the state driver what to do with the raw completion it returns:

- At transform time, whenever a statement is emitted verbatim into a state
  (`TransformContext::emit_statement`), compute the set of jumps that *escape*
  that statement (label-aware, see slice 1) and resolve each against the
  current `ctx.break_targets` / `ctx.continue_targets`. Store the result on
  the state as `inline_jumps: Vec<InlineJump>` where
  `InlineJump { kind: Break|Continue, label: Option<String>, terminator: StateTerminator }`
  and `terminator` is exactly what an explicit lowered `break`/`continue`
  would have produced (`Goto(target_state)`, or `LoopControl(target)` when
  `ctx.is_async && ctx.detect_for_await` — factor the existing
  `Statement::Break`/`Continue` arms' choice into one helper,
  `TransformContext::jump_terminator`, used by both).
- At run time, in each of the three drivers, when the state body returns
  `Completion::Break(label, _)` / `Completion::Continue(label, _)` and the
  state has a matching `inline_jumps` entry, substitute that terminator for the
  state's own. Every downstream mechanism (`align_generator_for_of_stack`
  closing for-of iterators, the async driver's `route_loop_control!` running
  transformed finalizers via `LoopControlTarget.try_depth` /
  `for_of_depth`) is reused unchanged. With no matching entry, behaviour is
  exactly today's.

Consequences: finalizers and `with` scopes are correct by construction (they
run natively before the completion surfaces); a `break` inside a `finally`
that overrides an earlier completion is correct; statements after the jump in
the same state body are skipped by the tree-walker as usual, and the state's
`Yield`/`Goto` terminator is *replaced*, not run.

## 4. Files to touch

- `src/interpreter/generator_transform.rs`
  - add `InlineJump`/`JumpKind`; add `inline_jumps: Vec<InlineJump>` to
    `GeneratorState` (two constructors: `new_state`, `create_simple_machine`
    — the latter always empty);
  - add `escaping_jumps(stmt) -> Vec<(JumpKind, Option<String>)>` (free fn next
    to `stmt_has_break_or_continue`, which is left untouched);
  - accumulate pending inline jumps in `TransformContext` from `emit_statement`,
    move them into the state in `finalize_current_state`;
  - add `TransformContext::jump_terminator` and reuse it in the explicit
    `Break`/`Continue` arms;
  - add `GeneratorState::inline_jump_terminator(&self, &Completion) -> Option<StateTerminator>`
    (or take `(kind, label)` to avoid a dependency on `Completion` in this
    file);
  - unit tests in the existing `#[cfg(test)] mod tests`.
- `src/interpreter/eval/generator_runtime.rs`: sync-generator driver
  (`terminator` bound at ~line 755, matched at ~873) and async-generator driver
  (bound at ~4140, matched at ~4294): shadow `terminator` after the
  `stmt_result` handling when the raw completion is `Break`/`Continue` with a
  table hit.
- `src/interpreter/eval.rs`: async-function driver (`stmt_result` handling at
  ~8759): consult the table **before** the existing raw-`Break`/`Continue`
  for-of heuristics; on a hit `route_loop_control!(target); continue;`. The
  heuristics stay as the no-entry fallback.
- New `test262-extra/*.js` files (see §6).
- `docs/`: no ADR (no new architectural boundary). Add a two-line note on
  inline jumps to the `Generators are compiled by generator_transform.rs`
  paragraph in `CLAUDE.md` Architecture Notes only if the reviewer wants it;
  `CONTEXT.md` needs nothing (no new domain term beyond the code-level type).

## 5. TDD slices

Build once: `cargo build --release -j4` (use `CARGO_TARGET_DIR` under `$TMPDIR`
if the shared `target/` is busy). Run gates as separate commands.

1. **Escape analysis (unit, red → green).** In
   `generator_transform.rs` `mod tests`, build ASTs directly and assert
   `escaping_jumps`:
   - `try { break; } finally {}` → `[(Break, None)]`; catch and finalizer
     bodies are also walked; `with (o) { break; }` → `[(Break, None)]`.
   - Depth rules: `while(1) { break; }` → `[]` (consumed natively);
     `while(1) { continue; }` → `[]`; `switch(x){ case 0: break; }` → `[]`;
     `switch(x){ case 0: continue; }` (no native loop) → `[(Continue, None)]`;
     `for(;;) { switch(x){ case 0: continue; } }` → `[]`.
   - Labels: `outer: while(1) { break outer; }` → `[]` (label consumed
     natively); `while(1) { break outer; }` → `[(Break, Some("outer"))]`;
     `continue outer` likewise; a label declared *inside* the statement shadows
     only within that statement.
   - Never descends into function/class declarations; ignores expressions.
   - Multiple distinct jumps are all reported once (dedupe by
     `(kind, label)`).
   Production code: `escaping_jumps` + `JumpKind`.
2. **Table population (unit).** Transform a body
   `switch(x){ case 1: try{break;}finally{f()} case 2: g(); break; case 3: yield 0; }`
   and assert the case-1 state carries an `InlineJump` for `(Break, None)` whose
   terminator is `Goto(after_switch)`, and that a state with no escaping jump
   has an empty table. Also assert an async-function machine
   (`transform_async_function`) records `LoopControl` with the right
   `try_depth`. Production code: `InlineJump`, `inline_jumps`,
   `emit_statement` accumulation, `finalize_current_state` hand-off,
   `jump_terminator`.
3. **Sync generator driver (end-to-end red → green).** New
   `test262-extra/generator-switch-yield-free-try-break-does-not-fall-through.js`:
   the issue's `t(1)` must yield `["f"]` (assert both that `f` ran and that
   `two` did not); plus variants: try/catch (`try { throw 0 } catch { break }`),
   try/finally where the finalizer replaces the jump
   (`try { break } finally { continue outer }`, and `finally { return }`),
   nested `try` inside `try`, and a non-jumping `try` that must still fall
   through (`case 1: try {} finally {}` → falls into case 2). Production code:
   the `generator_runtime.rs` sync driver substitution.
4. **Loops (end-to-end).** New
   `test262-extra/generator-loop-yield-free-try-break-continue.js`:
   `for`, `while`, `do-while`, labelled `continue outer`/`break outer`, and
   `for-of` (assert the iterator's `return()` is called exactly once on
   `break`, not on `continue`) with a yield-free `try { ... break/continue }
   finally { log }` before a `yield`. Expected (node): the `b`/`c` probes —
   `[0,1,"f0,f1,f2"]` and `[0,2,3,"f0,f1,f2,f3"]`. Same driver code as slice 3;
   this slice is the guard against for-of iterator-close regressions.
5. **`with` scope (end-to-end).** New
   `test262-extra/generator-switch-yield-free-with-break-does-not-fall-through.js`:
   `case 1: with (o) { l.push(k); break; }` yields `["1"]`; the `with` body
   still resolves `k` through the object; a `with` whose body does not break
   falls through. Expected to pass with no new production code (regression pin
   for the "`with` scope" concern in the issue).
6. **Labelled/outer jumps through nested natives (end-to-end).** New
   `test262-extra/generator-yield-free-nested-jump-escapes-clause.js`: a
   yield-free `case` containing `for(;;) { break outer; }` /
   `switch (y) { case 0: continue outer; }` /
   `while (1) { break; }` — the first two must jump to the transformed target,
   the last must stay native (negative control for false positives — the
   silent failure mode of the analysis).
7. **Async drivers.** New
   `test262-extra/async-generator-yield-free-try-break-does-not-fall-through.js`
   and `test262-extra/async-function-yield-free-try-break-does-not-fall-through.js`
   (`flags: [async]`, `includes: [asyncHelpers.js, compareArray.js]`, follow
   the #672 file `async-switch-yield-free-case-break-does-not-fall-through.js`).
   Cover the switch shape and the loop shape, plus (async function only) a
   yield-free `try { break }` nested inside a *yielding* `try/finally` in a
   loop, where the async driver's `route_loop_control!` must run the outer
   finalizer. Production code: the async-generator driver substitution
   (`generator_runtime.rs`) and the async-function driver hook (`eval.rs`).
8. **Refactor.** Fold the three driver substitutions to call one small helper
   if the duplication is noticeable; run `./scripts/lint.sh`.

## 6. Test surface

Targeted test262 runs (all must be identical before/after, compared against
`origin/main:test262-pass.txt` as the runner already does):

```
uv run python scripts/run-test262.py test262/test/language/statements/switch/
uv run python scripts/run-test262.py test262/test/language/statements/try/
uv run python scripts/run-test262.py test262/test/language/statements/for-of/
uv run python scripts/run-test262.py test262/test/language/statements/for/
uv run python scripts/run-test262.py test262/test/language/statements/while/
uv run python scripts/run-test262.py test262/test/language/statements/do-while/
uv run python scripts/run-test262.py test262/test/language/statements/with/
uv run python scripts/run-test262.py test262/test/language/statements/labeled/
uv run python scripts/run-test262.py test262/test/language/statements/break/
uv run python scripts/run-test262.py test262/test/language/statements/continue/
uv run python scripts/run-test262.py test262/test/language/statements/async-generator/
uv run python scripts/run-test262.py test262/test/language/expressions/generators/
uv run python scripts/run-test262.py test262/test/language/expressions/async-generator/
uv run python scripts/run-test262.py test262/test/language/statements/generators/
uv run python scripts/run-test262.py test262/test/language/statements/async-function/
uv run python scripts/run-test262.py test262/test/staging/   # generators live here too
uv run python scripts/run-test262.py test262-extra/
```

(`test262/` submodule may be empty in a fresh workspace:
`git submodule update --init --depth 1 test262 spec` first. `spec/` was
already initialised for this plan.)

Then the full suite once (`uv run python scripts/run-test262.py`), plus
`cargo test --release` (unit tests + `tests/`) and
`uv run python scripts/run-custom-tests.py`.

Not covered by test262 (hence `test262-extra/`, slices 3–7): the transform's
treatment of yield-free `try`/`with` jumps inside a state-machine body. test262
only exercises generators where the whole statement either yields or is
absent, so it never catches the dropped completion. Each new file's frontmatter
names `sec-runtime-semantics-caseblockevaluation` and/or
`sec-try-statement-runtime-semantics-evaluation` (and `sec-loopcontinues` /
`sec-with-statement-runtime-semantics-evaluation` where relevant) with an
`info:` block, following the #672 files.

No library harness or shim is involved.

## 7. Regression risk

- **Baseline movement.** The new path only fires when a state body returns a
  raw `Break`/`Continue` *and* the state has a table entry — i.e. exactly the
  inputs whose completion is dropped today. Any test262 test that passes today
  is not exercising that path unless its expectation was already accidentally
  satisfied by the fall-through; the targeted runs above are the check. Plan
  is not to touch `test262-pass.txt`.
- **Async-function driver.** It already has raw-`Break`/`Continue` heuristics
  keyed on `for_of_stack` (unlabelled `break` → innermost for-of; `continue`
  matched by label). The table is consulted first and is strictly more exact;
  the heuristics remain as fallback. Watch `language/statements/for-of/` and
  `for-await-of` (`async-generator/` `yield-star-*`), which lean on
  `align_generator_for_of_stack` / `route_loop_control!` and `try_depth` /
  `for_of_depth` correctness of the recorded targets.
- **Target-resolution correctness.** Targets are resolved from
  `break_targets`/`continue_targets` at `emit_statement` time. Every yield-free
  emission site funnels through `emit_statement`, and each state is filled under
  a single context (states are finalized before contexts are restored), so
  key collisions inside one state should not occur; add a `debug_assert!` on
  conflicting duplicate keys.
- **Shared machinery touched:** `generator_transform.rs` (all generator/async
  machines), three drivers. **Not touched:** `eval_expr`/`exec_statement` hot
  paths, `property.rs`, GC (`InlineJump` holds `String`/`usize` only — no
  `JsValue`, so no `gc_safepoint()`/`trace_object_fields` change), the
  exhaustive `ObjectKind` matches (no new variant, no new
  `StateMachineGenerator` snapshot field), the bytecode fast path
  (`bytecode_enabled` is off by default), Node-compat library harnesses.
- **Cost.** One extra walk of each verbatim-emitted statement at *transform*
  time (once per transform); guard with
  `!break_targets.is_empty() || !continue_targets.is_empty()` so generators
  with no enclosing target pay nothing. Zero cost on the run-time hot path (the
  table is consulted only when a raw `Break`/`Continue` surfaces).
- Sanity-check with the `perf-counters` build only if a test shows a
  transform-time cost concern; not expected.

## 8. Out of scope / follow-ups

- **Separate pre-existing bug found while planning (not fixed here):** a
  *yielding* `try` whose body does `break`/`continue` leaves the transformed
  try in generators without running its finalizer, because the sync and async
  **generator** drivers collapse `LoopControl` into `Goto` and never route
  through pending finalizers:
  `for (...) { try { yield i; if (i==1) break; } finally { log.push(i) } }` →
  jsse `[0,1,"f0"]`, node `[0,1,"f0,f1"]`; `continue outer` out of a yielding
  try runs no finalizers at all. Async *functions* are correct (the driver has
  `route_loop_control!`/`pending_loop_control`). Consequence for this plan: an
  inline jump behaves **identically to an explicit lowered jump** in every
  driver — including when it crosses an enclosing transformed `try` — so this
  PR neither introduces nor worsens it, and the issue's own shapes (no
  transformed try crossed) are fixed completely. Follow-up issue to file after
  this PR: route `LoopControl` through un-entered finalizers in both
  generator drivers (`current_try_stack[target.try_depth..]`, keep
  `align_generator_for_of_stack` for for-of closing, add `pending_loop_control`
  to the two `IteratorState` snapshot variants and to `TryExit`, drop the
  `is_async && detect_for_await` gate on the explicit `Break`/`Continue` arms).
  Post a comment on #669 recording this finding and the decision to split it.
- Making `stmt_has_break_or_continue` label-aware or lowering `try`/`with`
  (the issue's suggested route) — superseded by inline-jump capture; not done.
- Removing the async driver's `for_of_stack` raw-`Break`/`Continue` heuristics
  now that the table makes them mostly redundant.
- #670 (`for-in` with `yield`), #671 (`yield` in a `case` test),
  #665 (`await using` disposal draining), and the
  `SentValueBindingKind::InlineYield` replay backstop (#625).
- Any formatting/refactor of unrelated generator-transform code; baseline
  file updates.

## 9. Delivery

- PR title (squash subject): `fix(generators): capture escaping break/continue from yield-free try/with in state-machine bodies (#669)`.
- Implementation stage: `git rm PLAN.md` before opening the PR; no
  `test262-pass.txt` edits; commit messages end with the required attribution
  line.
