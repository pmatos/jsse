# `yield` in a for-in/for-of *assignment*-form head pattern

Issue #687 (the standing `await_value` blocking-fallback audit) found that
`for await ([x = yield] of it)` inside an async generator — an
`ForInOfLeft::Pattern` head (an assignment target: no `var`/`let`/`const`),
as opposed to the `ForInOfLeft::Variable` (declaration) head #727/#753
already cover — never suspended correctly. Confirmed live via direct
instrumentation of the tree-walker's `exec_for_of_loop`: 37 hits across 13
test262 files, all `for await ([pattern = yield ...] of it)` /
`for await ({pattern = yield ...} of it)` inside an async generator, none
caught by test262 itself (those files assert final *values*, not
concurrent ordering).

## Two independent gaps, same shape as #727/#753's `Variable` case

1. **Detection never looked at a `Pattern` head at all.**
   `for_in_of_head_contains_yield` (`generator_analysis.rs`, renamed from
   `for_in_of_variable_head_contains_yield` since it's no longer
   variable-only) hard-coded `ForInOfLeft::Pattern(_) => false`. A for-of/for-in
   statement whose *only* suspension is a `yield` in its assignment-form head
   pattern was invisible to `contains_yield`/`contains_suspension`, so the
   statement never routed into `transform_for_in_of_loop` — it stayed a raw
   `Statement::ForOf`/`ForIn`, tree-walked by `exec_for_of_loop`, which runs
   the loop's `Await(nextResult)` step through the blocking `await_value`
   fallback with no suspend/resume awareness at all (issue #687's actual
   subject). Also: `analyze_statement`'s own `ForInOfLeft::Pattern(_) => {}`
   arms (`ForIn`/`ForOf`) were literal no-ops, unlike the `Variable` arm right
   above each, which calls `analyze_pattern_expressions` to register the
   pattern's embedded `yield` as a counted yield point.
2. **Even once detected, `ForOfHead`'s single non-suspending bind call can't
   run a yield-containing pattern through it directly.** At runtime,
   `StateTerminator::ForOfHead`'s `ForInOfLeft::Pattern` arm calls
   `self.assign_to_for_pattern(pat, val, &term_env)` and only distinguishes
   `Completion::Throw`; any other abrupt completion (in particular
   `Completion::Yield`) falls into a silent `_other => {}` catch-all — exactly
   the "silently drop the `Completion::Yield` and bind the wrong value" hazard
   #727/#753's own desugar was built to avoid for the declaration form.

## Decision

Mirror #727/#753's `Variable`-head desugar for the `Pattern` (assignment)
head, landing both layers together (shipping either alone is unsafe: layer 1
alone routes a yield-containing pattern straight into `ForOfHead`'s
silently-dropping bind call; layer 2 alone is simply never reached):

- **Layer 1 (detection):** widen `for_in_of_head_contains_yield`'s `Pattern`
  arm to `pattern_contains_yield(pattern)`, and fill in `analyze_statement`'s
  two no-op `ForInOfLeft::Pattern` arms with
  `analyze_pattern_expressions(pattern, analysis, ctx)` — the same call the
  adjacent `Variable` arm already makes.
- **Layer 2 (desugar):** `transform_for_in_of_loop`'s existing
  `rewritten_left` block (which calls `hoist_suspending_pattern` for a
  `Variable` head, producing a synthesized `let <pattern> = $tmp;`) gains a
  sibling `hoist_suspending_pattern_assignment` for a `Pattern` head,
  producing a synthesized `<pattern> = $tmp;` — a plain
  `Statement::Expression(Expression::Assign(..))`, not a declaration, using
  the existing `pattern_to_expr` conversion (already imported and used by
  `emit_pattern_assignment` for the same shape) to turn the `Pattern` back
  into an assignment-target expression. `ForOfHead` then binds a trivial
  `Pattern::Identifier($tmp)` instead, and the real pattern — prepended to the
  loop body via the same `left_param_synth`/`effective_body` mechanism the
  `Variable` case already uses — is handled by the *ordinary* per-statement
  transform dispatch for `Expression::Assign`.

## Why the ordinary per-statement dispatch is enough, even for array patterns

Unlike `hoist_suspending_pattern`'s await path — which is gated by
`pattern_needs_await_lowering` and so only fires for a shape
`pattern_lowering_supported` actually accepts — `hoist_suspending_pattern_assignment`
is gated purely on `pattern_contains_yield`, with **no shape restriction**.
This is deliberate and was the key design question for this ADR: the
confirmed live repro (§ below) uses an *array*-pattern head
(`[x = yield]`), and `pattern_lowering_supported`/`pattern_needs_assignment_lowering`
categorically reject `Pattern::Array` (`generator_analysis.rs`) — they cannot
decompose it into explicit states. Unhooking the hoist from that shape gate
looked risky at first (would an unsupported shape's synthesized
`<pattern> = $tmp;` statement be handled correctly once it lands in the body?)
until tracing what "ordinary per-statement dispatch" already means for an
`Expression::Assign` with an `Expression::Array`/`Expression::Object` LHS:

- A **supported** shape (an object pattern, `pattern_needs_assignment_lowering`
  true) hits `transform_yielding_expression`'s dedicated
  `Expression::Assign` arm guarded on `pattern_needs_assignment_lowering`,
  which calls `lower_pattern_assignment` and fully decomposes it into states
  — no replay, same as the `Variable`-head object-pattern case.
- An **unsupported** shape (an array pattern) falls through that guard to the
  *generic* `Expression::Assign` arm — but only because the guard is a Rust
  `match` arm precondition, not an entry into some separate code path; the
  *statement* containing it (`stmt_has_suspension`) was already recognized as
  suspending by layer 1, so the compiled function still routes this statement
  through the InlineYield replay backstop (ADR-2026-09-21-2157), confined to
  just this one statement in the loop body, on each resume. Verified directly
  (`[x = yield] = []` as a bare statement, outside any loop, inside a sync
  generator): the sent value flows through correctly. This is exactly the
  same "decompose when the shape allows, replay when it doesn't" split the
  `Variable`-head desugar already established (`lower_pattern_binding` vs.
  native InlineYield replay) — extended here to the assignment form, using
  its own already-correct assignment-side counterpart.

The desugar therefore doesn't need to special-case shape at all: the mere act
of moving the pattern out of `ForOfHead`'s single non-suspending bind call
and into the body is what fixes the bug, regardless of which downstream
mechanism (full decomposition or replay) ends up evaluating it.

## What's deliberately left alone

- **`await`-only assignment-form heads** (`for await ([a = await x] of it)`):
  `for_in_of_variable_head_contains_await`'s own `Pattern` arm stays
  `false`, untouched by this change — an array-pattern `await` default in a
  for-of/for-in head is issue #725's territory (currently open, owned
  elsewhere), not this one. `hoist_suspending_pattern_assignment` is gated on
  `pattern_contains_yield` only, so it never fires for an await-only pattern.

  **Superseded by ADR-2026-10-02-0000** (issue #788): both are now widened —
  `for_in_of_variable_head_contains_await`'s `Pattern` arm calls
  `pattern_needs_await_lowering`, and `hoist_suspending_pattern_assignment`
  also fires on it — closing this gap for both object and array shapes.
- **`ForOfInit`'s own `left` field** is left un-rewritten (kept as the
  original, real pattern) for the same reason #726/ADR-2026-09-21-2143
  established for the `Variable` case: it only supplies `BoundNames` for the
  head's TDZ pre-declaration environment. `for_of_head_lexical` returns
  `None` for any non-`Variable` left, so this is a no-op for the
  assignment-form case — an assignment target declares no new bindings, so
  there's no TDZ environment to seed in the first place.
- **`eval.rs:1026`'s bare `Expression::Await`** and the rest of the #687
  audit's other two call sites — out of scope for this ADR; see the issue.

## Confirmed fixed

The issue's own ordering repro (array-pattern head, default never fires) now
matches Node exactly: `sync-end` logs immediately after the first `it.next()`
call returns, instead of after the entire first loop iteration (including a
second iterator `next()` call) ran inside it. A second scenario — a value
that *does* trigger the default, so the pattern's `yield` actually fires and
a real `.next(sentValue)` resume happens — confirms no replay/double-iteration
regression: the async iterator is acquired exactly once and stepped exactly
once per element, mirroring #753's own counting-iterator test for the
`Variable`-head sibling. Both are pinned as `test262-extra` regressions
(not test262-coverable: test262's own files in this shape assert values,
not concurrent ordering or call counts).
