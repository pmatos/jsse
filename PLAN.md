# Plan: issue #788 — async generator `.return()` during a pending `Await`

> Revision history: an earlier revision concluded there was no live defect
> (based on re-running the issue's literal for-of-*declaration*-head repro,
> which no longer reproduces). A parallel investigation found live triggers
> for the same class of bug via two different, still-unfixed syntactic
> shapes. This revision supersedes that conclusion with a real production
> fix covering both.

## 1. Problem restated

The issue claims `.return()` on an async generator parked inside an `Await`
settles within a couple of microtask turns instead of staying pending
forever, and that the outer for-of iterable's own iterator never gets
`.return()` called even though the loop is abandoned.

**What's already fixed:** the issue's own literal repro (an object- or
array-*declaration*-pattern default at a for-of head, e.g. `for (let { b =
await new Promise(() => {}) } of outer) {}`) no longer reproduces on this
branch — jsse now matches `node` exactly (verified: the literal repro, an
array-pattern twin, a genuinely-interrupted-loop variant, and a six-site
sweep of other park points). Several "lower await in destructuring
defaults" fixes (#772, #773, #789) landed on this branch after #788 was
filed and incidentally closed it. The transcript quoted in the issue is
what spec-correct behavior produces when the await never suspends at all
(the pre-#772/#773/#789 defect) and the loop runs to completion before the
queued `.return()` is serviced — not evidence of an early interrupt.

**What's still live — two confirmed triggers for the same bug class**, both
on this branch's release binary, both diverging from `node`, both because a
suspending construct is evaluated by the tree-walker's blocking
`await_value` fallback (`src/interpreter/eval.rs`, `Expression::Await` arm)
instead of being lowered into real suspension states. `await_value`
busy-drains the microtask queue inline looking for its own promise to
settle (`:10161-10347`); for a non-agent thread `await_deadline` is `None`,
so once the queue runs dry with the promise still pending it **gives up and
fabricates `undefined`** (`:10345`), letting the body race forward. Any
`.return()`/`.next()` drained as an "unrelated" microtask during that busy
loop was correctly deferred by the queue-occupancy gate at the time
(`execution_state` really is `Executing` for that instant) — the actual bug
in both cases is that the generator/async-function was never supposed to
resume from the stuck `await` at all:

- **Trigger A — a bare array-assignment-pattern default**, e.g. `[a = await
  new Promise(() => {})] = []`, anywhere in a generator/async-function/
  async-generator body (not inside a `for`-head). Reproduces identically in
  a plain `async function` (verified) and inside an async generator with a
  queued `.return()` racing it (verified). `docs/adr/2026-09-22-1815-
  destructuring-assignment-lowering.md` ("What this change deliberately
  leaves imprecise") explicitly deferred this, reasoning it would need "new
  interpreter-internal helpers to keep a `GetIterator`/`IteratorStep` record
  alive and closed-exactly-once across states." That machinery already
  exists — built for the *declaration*-pattern case
  (`ArrayPatternIterOp`/`StateTerminator::ArrayPatternIter`,
  `generator_transform.rs:100-320`, used by `lower_array_pattern_binding`,
  `:2128-2216`) — so the ADR's stated reason to defer no longer holds.
  `lower_pattern_assignment` (`:2345-2364`) special-cases only
  `Pattern::Object`; a `Pattern::Array` left side falls through to
  `emit_pattern_assignment` unchanged, so any `await` inside it reaches
  `eval_expr` as part of one atomic statement and hits the blocking
  fallback.

- **Trigger B — a `for`/`for-in`/`for-of` head using the *assignment* form**
  (no `let`/`const`/`var`), e.g. `for ([a = await never] of outer) {}` or
  `for ({ b = await never } of outer) {}` — **both object and array
  shapes**, verified diverging from `node` the same way (confirmed live for
  both). Root cause is distinct from Trigger A:
  `for_in_of_variable_head_contains_await` (`generator_analysis.rs:872-884`)
  unconditionally returns `false` for `ForInOfLeft::Pattern` (the
  assignment-form head), regardless of pattern shape — only the
  *declaration*-form (`ForInOfLeft::Variable`) arm checks
  `pattern_needs_await_lowering`. Consequently
  `hoist_suspending_pattern_assignment` (`generator_transform.rs:1998-2013`)
  only hoists an assignment-form head's pattern out of `ForOfHead`'s single
  non-suspending bind call when it contains a `yield`
  (`if !pattern_contains_yield(pattern) { return None; }`, `:2003-2005`) —
  never for an `await`-only default — so the pattern is bound in place by
  `ForOfHead`'s own call, which has no suspension path at all. This is
  literally #788's own repro shape, just with the head's target already
  bound (assignment) rather than freshly declared.

Both are one bounded shape each, fixable by mirroring an existing sibling
mechanism — consistent with how every other suspending-construct gap in
this area has been closed (#772, #773, #775, #781, #783, #784, #785, #787,
#789, #790: one PR per shape).

## 2. Spec basis

- `Await` (spec.html:51047-51081, `await`): never reads or writes any
  generator-execution state; a construct containing an `Await` that doesn't
  suspend through the state machine is the defect — the interpreter's
  blocking busy-loop fallback is not a form of suspension the spec
  recognizes.
- `IteratorDestructuringAssignmentEvaluation`, `AssignmentElement :
  DestructuringAssignmentTarget Initializer?` (spec.html:21172-21197,
  `sec-runtime-semantics-iteratordestructuringassignmentevaluation`):
  governs per-element evaluation order for an array-assignment pattern.
  Step 1: if the target is *not* itself a nested pattern, its reference
  (`lRef`) is evaluated **first**, before the iterator step and before the
  `Initializer` — "Left to right evaluation order is maintained by
  evaluating a DestructuringAssignmentTarget that is not a destructuring
  pattern prior to accessing the iterator or evaluating the Initializer."
  Governs Trigger A's fix (mirroring `lower_pattern_assignment_property`'s
  existing member-expression handling, `:2410-2423`).
- `ArrayAssignmentPattern` productions (spec.html:20990-21069,
  `sec-runtime-semantics-destructuringassignmentevaluation`): the overall
  `GetIterator`-then-walk-then-`IteratorClose`-on-abrupt-completion shape,
  already implemented generically by `ArrayPatternIterOp::{Init,Step,Drain,
  Finish}` for the declaration form; Trigger A's fix reuses the same
  terminators with assignment-form leaves.
- `ForIn/OfBodyEvaluation` (spec.html:22388-22465,
  `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-
  lhskind-labelset`), step 5.a.i (spec.html:22417-22420): when `lhsKind` is
  `assignment` and the head is destructuring, each iteration runs
  `DestructuringAssignmentEvaluation of assignmentPattern with argument
  nextValue` — the *same* AO as Trigger A's bare-expression case, just
  invoked once per iteration from the loop head instead of from
  `Expression::Assign`. Governs Trigger B's fix.
- The async-generator `.return()`-queuing clauses cited in the original
  issue (`sec-asyncgenerator-prototype-return`, `sec-asyncgeneratorenqueue`,
  the `[[AsyncGeneratorQueue]]` non-empty-iff-executing-or-draining-queue
  invariant at spec.html:50604) remain the reason this matters: already
  correctly implemented (per `docs/adr/2026-09-21-2246-async-generator-
  awaiting-return-parking.md`) for every site that actually parks. Both
  fixes below make their respective shape genuinely park, which is what
  lets that already-correct queuing logic do its job.

## 3. Files to touch

**Trigger A:**
- `src/interpreter/generator_analysis.rs`: widen `pattern_lowering_supported`'s
  `Pattern::Array` guard (`:1098-1102`) to also match
  `PatternLoweringForm::Assignment`; update the now-stale doc comments on
  `pattern_lowering_supported` (`:1071-1085`) and
  `pattern_needs_assignment_lowering` (`:1129-1137`) that assert arrays are
  excluded.
- `src/interpreter/generator_transform.rs`: add a `Pattern::Array(elements)
  => lower_array_pattern_assignment(elements, source, ctx)` arm to
  `lower_pattern_assignment` (`:2345-2364`); new
  `lower_array_pattern_assignment` function mirroring
  `lower_array_pattern_binding` (`:2128-2216`) in full — elision, rest,
  nested-pattern recursion into `lower_pattern_assignment`, and a
  non-pattern leaf (identifier or member-expression, optionally wrapped in
  `Pattern::Assign`) capturing its target reference before that element's
  `Step` terminator (mirroring `lower_pattern_assignment_property`'s
  existing member-expression idiom). This must land as one unit, not
  split across slices: `pattern_lowering_supported`'s gate and
  `lower_array_pattern_assignment`'s arm must agree on exactly what's
  supported at every point, or the gate can route an array shape the arm
  can't yet lower.

**Trigger B** (depends on Trigger A's array arm existing):
- `src/interpreter/generator_analysis.rs`: `for_in_of_variable_head_contains_await`
  (`:872-884`) — change the `ForInOfLeft::Pattern(_) => false` arm to
  `ForInOfLeft::Pattern(pattern) => pattern_needs_await_lowering(pattern)`,
  mirroring the `Variable` arm immediately above it.
- `src/interpreter/generator_transform.rs`: `hoist_suspending_pattern_assignment`
  (`:1998-2013`) — widen its guard from `!pattern_contains_yield(pattern)`
  to also hoist when `pattern_needs_await_lowering(pattern)` is true,
  reusing the exact same hoist-to-body-statement strategy already used for
  `yield`. Gating through `pattern_needs_await_lowering` (not a looser
  check) is what keeps an unsupported shape (e.g. object rest beside a
  suspending sibling, #771) correctly un-hoisted — `await` has no
  `InlineYield`-style replay backstop, so only hoisting what
  `lower_pattern_assignment` can actually lower is load-bearing, not
  cosmetic.

**Both:**
- `test262-extra/` — new regression files per the TDD slices below.
- `docs/adr/` — new ADR recording both decisions (array-assignment patterns
  and assignment-form for/for-in/for-of heads now get the same
  state-machine lowering as their declaration-form counterparts, reusing
  `ArrayPatternIterOp` and the existing hoist-to-body strategy), plus a
  short superseding note on `docs/adr/2026-09-22-1815-destructuring-
  assignment-lowering.md`'s "What this change deliberately leaves
  imprecise" bullet (no longer true once Trigger A lands) and on that same
  ADR's "for-of/for-in heads were never actually affected" note (no longer
  true once Trigger B lands).
- No `CONTEXT.md` change — no new vocabulary, just existing mechanisms
  (`ArrayPatternIterOp`, the hoist-to-body strategy) gaining callers.

## 4. TDD slices

1. **Slice 1 (production, Trigger A) — the full array-assignment mirror.**
   Implement `lower_array_pattern_assignment` completely (identifier,
   elision, rest, nested pattern, member-expression leaf) and the
   `pattern_lowering_supported` widening in one change, per the "must land
   as one unit" note in section 3.
   Tests (`test262-extra/`, written from section 2's clauses first, run
   against current `main` to confirm **red**, i.e. diverging from `node` as
   already verified during triage, then turned **green** by the production
   change):
   - `async-generator-array-assignment-pattern-default-await-suspends.js` —
     the core repro: `[a = await deferred] = []` genuinely parks; a
     concurrent `.return()` on the enclosing async generator stays queued
     (not settled) until the generator reaches a real yield or completes,
     mirroring the already-verified "stays pending" shape. Cover the plain
     `async function` case too (not only async-generator), since
     `lower_pattern_assignment` is shared infrastructure.
   - `async-generator-array-assignment-pattern-completion-value.js` —
     `([a = await x] = arr) === arr` (`AssignmentExpression` step 7.c,
     "Return rval" — ADR-2026-09-22-1815's point that the new arm must bind
     the source temp to the caller's binding, not a destructured result).
   - `async-generator-array-assignment-pattern-abrupt-close.js` — a
     *rejected* default `await` closes the array-pattern iterator exactly
     once (the suspension-specific abrupt path, distinct from a
     synchronous throw mid-walk, which slice adds elision/rest coverage
     for).
   - `async-generator-array-assignment-pattern-elision-rest-nested.js` —
     elision, a rest element, and a nested pattern element (`[[a] = await
     x] = []`, `[{a} = await x] = []`), mirroring the existing declaration-
     side `async-generator-yield-in-array-pattern-default-loop.js`'s
     structure.
   - `async-generator-array-assignment-pattern-member-expression-order.js`
     — `[obj.prop = await x] = []` and `[o[await k]] = arr` (the computed
     key contains its own `await`), with an observable side-effect log
     proving the reference is captured before the iterator step and before
     the default, per the spec note at spec.html:21196.

2. **Slice 2 (production, Trigger B) — assignment-form loop heads.**
   Widen `for_in_of_variable_head_contains_await` and
   `hoist_suspending_pattern_assignment` as described in section 3.
   Tests (`test262-extra/`, same red-then-green discipline, both already
   verified diverging from `node` during triage):
   - `async-generator-for-of-head-assignment-array-pattern-await-
     suspends.js` — `for ([a = await never] of outer) { yield a; }`: a
     queued `.return()` stays pending while parked, and is delivered at the
     real `yield` (closing `outer` exactly once), mirroring this issue's
     own repro shape but in assignment-head form.
   - `async-generator-for-of-head-assignment-object-pattern-await-
     suspends.js` — the object-shape twin (`for ({ b = await never } of
     outer) { yield b; }`); same assertions. This one is a value-correct,
     ordering-imprecise case today (per ADR-2026-09-22-1815's "for-of/for-in
     heads were never actually affected" note) — this test is what
     upgrades it to spec-correct ordering, not a hang fix.

Both slices' tests are written from the spec clauses in section 2, run
against current `main` first to confirm red (already empirically confirmed
during triage for every case above), then turned green by the
corresponding production change.

## 5. Test surface

- No existing test262 coverage overlaps Trigger A: `test262/test/language/
  expressions/assignment/dstr/` and `test262/test/language/statements/
  variable/dstr/` were checked — the only `*await*`-named files there
  (`ary-ptrn-elem-id-static-init-await-{valid,invalid}.js`,
  `obj-ptrn-elem-id-static-init-await-{valid,invalid}.js`) test `await` as
  a valid/invalid *identifier name*, not an `await` *expression*'s
  suspension behavior.
- Targeted test262 run as a sanity check that both widened lowerings don't
  disturb sibling coverage: `uv run python scripts/run-test262.py
  test262/test/language/expressions/assignment/
  test262/test/language/statements/variable/dstr/
  test262/test/language/statements/for-of/
  test262/test/language/statements/for-in/
  test262/test/language/statements/async-generator/dstr/
  test262/test/language/expressions/async-generator/dstr/` — includes the
  ~156 `dstr/*-yield-expr` tests #724 flagged as passing via `InlineYield`
  replay (must keep passing unchanged: both widened predicates gate on
  `pattern_needs_await_lowering`/`pattern_contains_await`, not `_suspension`,
  so a pattern with only a `yield` default is untouched by either change
  regardless of shape).
- New coverage: run every new `test262-extra/` file listed in section 4 via
  `uv run python scripts/run-test262.py <files...>`.
- Full custom suite: `uv run python scripts/run-custom-tests.py`.
- `cargo test --release` and `./scripts/lint.sh` as the standard quality
  gate (run separately, not `&&`-chained).
- Full `test262/` run is appropriate here (both are engine-behavior
  changes) but **do not** pass `--update-baseline` — that's a `main`-branch
  roll-forward operation, not part of this PR.

## 6. Regression risk

- **The ~156 `dstr/*-yield-expr` tests** (#724's closing note) currently
  pass via `InlineYield` replay for pure-`yield` assignment-pattern
  defaults. Safe by construction for both slices (see section 5); still
  worth running explicitly since #724 flagged it as the specific regression
  to watch for any future touch here.
- **Trigger B changes already-"working" behavior**, not hanging behavior:
  per ADR-2026-09-22-1815, assignment-form for/for-in/for-of heads with an
  `await` default currently produce *correct values* via the blocking
  fallback, just with imprecise job ordering. Widening the hoist makes them
  genuinely suspend instead — a user-observable change in *when* a
  `.then()` callback fires relative to other jobs, even though the issue's
  own repro treats that imprecision as the bug. Low risk of breaking a
  value-correctness test, but worth calling out explicitly in the PR
  description since it's a behavior change to a path that wasn't hanging.
- **Shared transform infrastructure**: `lower_pattern_assignment`,
  `ArrayPatternIterOp`, and the hoist-to-body strategy are used by every
  suspending-body construct (sync generators, async functions, async
  generators alike) — neither fix is async-generator-specific, so both
  should be verified against a plain `async function` repro too (slice 1
  already does; slice 2's for-of-head shape applies identically to a plain
  `async function` and should get the same coverage).
- **`ArrayPatternIterOp`'s existing (declaration-side) behavior**: unchanged
  by slice 1 — only a new caller is added, not a change to the terminator's
  interpreter-side execution in `generator_runtime.rs`/`eval.rs`.
- **`test262-pass.txt` baseline**: both are genuine behavior changes and
  could move the pass count in the directories listed in section 5. Per
  project rules, this plan does **not** roll the baseline forward.
- **Bytecode fast path**: unaffected — generator/async-generator/async-
  function bodies containing a suspension run through the tree-walker's
  state-machine driver regardless of the bytecode feature flag.
- **GC rooting**: no new root-scope shape — both fixes reuse existing
  temp-var/terminator plumbing already rooted correctly for the
  declaration-side and object-assignment-side cases.

## 7. Out of scope

- **Object-rest beside a suspending sibling** (`{a = await 1, ...rest} =
  x`), tracked as issue #771 — unrelated pre-existing gap for both
  declaration and assignment forms, not reopened here. Both slices above
  correctly leave it un-hoisted/unsupported via `pattern_lowering_supported`.
- **Other remaining blocking-`await_value` sites** identified during
  triage but not reproduced/fixed here, each a candidate for its own issue
  mirroring this project's established one-shape-per-PR pattern:
  - `for await` in the *naive* (non-compiled) driver's own fallback
    (`src/interpreter/exec.rs:2364-2390`) — needs confirming whether it's
    actually reachable from a compiled async-generator context before
    treating it as live.
  - Frame-exit disposal, loop-control-crossing-for-of, and delegated
    `yield*` abrupt exits still blocking in places documented as "known
    boundaries" in `docs/adr/2026-09-21-2015-async-generator-frame-exit-
    disposal.md`, `docs/adr/2026-09-22-2340-async-generator-for-of-unwind-
    suspends.md`, and related ADRs — several tracked under open issue #665
    ("`await using` in try/loop/switch bodies and async generators still
    drains microtasks inline at disposal").
- **Removing/marking-dead the unreachable `Executing` match arms** in the
  `.next`/`.return`/`.throw` entry points — already a documented, deliberate
  boundary (ADR-2026-09-21-2246); not this issue's scope.
- **Any change to `StateMachineExecutionState` or the generic
  `async_gen_enqueue`/`async_gen_process_queue` queue-occupancy gating** —
  already correct; both fixes are entirely at the lowering-coverage layer,
  not the queue-gating layer.
