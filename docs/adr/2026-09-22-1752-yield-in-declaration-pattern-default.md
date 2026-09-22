# `yield` in a var/let/const pattern default: three independent gaps

Issue #727: `function* g(){ var {a = yield 1} = {}; return a }` never
suspended — `it.next()` returned `{"done":true}` immediately instead of
`{"value":1,"done":false}`. Same for `var [a = yield 1] = []` and for
`async function*`. Filed alongside #709/#724, which fixed the equivalent
`await` cases.

## Three gaps, not one

The implementation plan for this issue anticipated two of these; tracing why
the fixed detectors still produced a single-state machine surfaced a third.

1. **`contains_yield`'s and `contains_suspension`'s `Statement::Variable`
   arms never looked at a declarator's *pattern***, only its `init`
   expression, so a statement whose only suspension lived in a pattern
   default was never recognized as suspending at the per-statement level.
2. **`pattern_needs_lowering` (`generator_analysis.rs`) only triggered on
   `await`.** Even once a statement was recognized as suspending, a
   yield-only object pattern stayed off `lower_pattern_binding`'s
   state-machine path and kept running on the plain tree-walker, unsuspended.
3. **`analyze_generator_body`'s yield-point collector — which decides
   whether a generator's state machine is built at all, versus taking the
   single-state "simple machine" shortcut
   (`transform_generator_inner_opts`'s `analysis.yield_points.is_empty()`
   check) — walked a declarator's `init` but never its pattern either.** A
   sync generator whose only `yield` lived in a pattern default skipped
   state-machine construction *entirely*, regardless of fixes 1 and 2: the
   whole function ran as a single tree-walked state with no yield points to
   suspend at. This gap was not in the plan; it was found by transforming
   the exact repro and observing a 1-state machine with `num_yields == 0`
   even after 1 and 2 were fixed.

## Decision

**Widen detection in all three places, uniformly, by walking the pattern's
embedded expressions** (computed keys, member-expression targets, default
Initializers) — mirroring the existing `pattern_contains_await`/
`pattern_any_expr` walker for the first two, and adding a parallel
side-effecting walk (`analyze_pattern_expressions`) for the third, since
`analyze_statement`'s existing `collect_pattern_vars` only gathers bound
names and deliberately discards embedded expressions.

- `pattern_contains_yield` (new) feeds `contains_yield`'s `Statement::Variable`
  arm.
- `pattern_needs_lowering` becomes `pattern_contains_suspension(pattern) &&
  pattern_lowering_supported(pattern, false)` — an await-gate widened to a
  suspension-gate. Object patterns (the shape `lower_pattern_binding`
  supports) with a yield-only default are now lowered exactly like an
  awaiting one; array patterns and an object rest beside a suspending
  sibling are still declined by `pattern_lowering_supported` regardless of
  which kind of suspension is inside.
- `analyze_pattern_expressions` feeds `analyze_statement`'s `Statement::Variable`
  arm, so a pattern-only `yield` registers a `YieldPoint` and the generator
  no longer takes the single-state shortcut.

**Widening `pattern_needs_lowering` was enough for `contains_suspension` to
pick up the object-pattern async-generator case for free** — it already
called `pattern_needs_lowering`, so no separate edit was needed there
(the plan expected one). Confirmed by testing the async-generator repro with
only the `pattern_needs_lowering` widening applied: it already suspends and
resumes correctly.

**`contains_suspension` still needed one targeted addition**, found through
the plan's own for-loop test, but only for the *array*-pattern case in an
*async* context: `contains_suspension`'s `Statement::Variable` arm gained
`|| pattern_contains_yield(&d.pattern)` (not `pattern_contains_suspension`).
An array pattern's `yield` isn't itself lowered (`pattern_needs_lowering` is
still false for it — unsupported shape), but the *enclosing* construct (a
`for` loop, in the reported case) must still recognize the suspension so the
loop is split into per-iteration states; otherwise the whole loop stays one
tree-walked statement and the InlineYield replay re-runs it from the top on
resume, duplicating every earlier iteration's side effects. Using
`pattern_contains_suspension` here instead (as originally drafted) would
have caught this but also regressed
`test_unsupported_pattern_shapes_stay_on_the_tree_walker`: an *await*-only
array pattern in a plain async function relies on staying on the
single-state "simple machine" and blocking synchronously on `await_value` —
that path needs no state split, since blocking is a legitimate
implementation strategy for `await` (unlike `yield`, which cannot block: it
must return control to the caller). `pattern_contains_yield` triggers only
for the strictly-generator case that actually needs it.

## `bind_pattern` stops swallowing `Completion::Yield`

Even with detection and lowering fixed for object patterns, array patterns
(`var [a = yield 1] = []`) still silently produced `undefined` and completed
immediately — including as the *only* statement in a generator body, no loop
involved. Array patterns are never lowered (`pattern_lowering_supported`
rejects `Pattern::Array`), so the raw `yield` reaches `bind_pattern`'s
tree-walking evaluation of the default directly. `bind_pattern` discarded
any `eval_expr` completion that wasn't `Normal`/`Throw` (three `_ =>
JsValue::UNDEFINED` catch-all arms — in the `Pattern::Assign` default, the
object pattern's computed-key arm, and its `var`-fast-path default),
silently destroying the `Completion::Yield` before it could reach the
generator runtime's InlineYield fallback (`eval/generator_runtime.rs`,
`SentValueBindingKind::InlineYield`) at all.

**Change `bind_pattern`'s return type from `Result<(), JsValue>` to
`Completion`**, using the existing `propagate!`/`IntoAbrupt` seam
(`types.rs`, from #623) to unwind `Throw` and `Yield` alike with one
spelling. The array-pattern arm mirrors `destructure_array_assignment`'s
(the destructuring-*assignment* form's, already correct) existing split
between an abrupt `Throw` — closes the iterator now, per
§13.15.5.2 IteratorBindingInitialization — and a `Yield` — the iterator is
stashed on `pending_iter_close` instead, since this tree-walked attempt is
abandoned (the InlineYield fallback replays the whole statement from the top
on resume, so the iterator is never resumed either) and needs to be closed
*eventually*, the same bookkeeping #724 already established for
destructuring-assignment array patterns.

**~16 call sites** across `exec.rs`/`eval.rs`/`eval/generator_runtime.rs`
were updated to match on `Completion` instead of `Result`. The declaration
path (`exec_variable_declaration`, `bind_pattern`'s own recursive calls)
propagates `Yield` like any other abrupt completion. The for-in/for-of head
binds and the two catch-binding sites keep discarding non-`Throw`
completions (narrowed from a bare `if let Err(e) = ...` to `if let
Completion::Throw(e) = ...`) — those call sites' own suspension detection is
issue #726's territory, out of scope here, and the catch-binding sites
already dropped `Throw` too before this change (#739's finding).

## What this still does not cover

- **Full array-pattern iterator-safety under suspension** (#725): the
  iterator record isn't held live across a real suspension the way the
  compiled state machine holds one for a lowered object pattern; it's
  abandoned and (eventually) closed via `pending_iter_close`, then
  re-created from scratch on resume. Confirmed via a side-effect-ordering
  probe: `var [a = yield 1] = (n++, [])` inside a generator resumes with
  `n === 2` — the source expression is evaluated twice — a pre-existing,
  accepted limitation of the InlineYield replay fallback, not a regression
  introduced here.
- **`catch ({a = yield 1})` and `for (var {a = yield 1} of x)` /
  `for (var {a = yield 1} in x)`** (#726): `bind_pattern`'s signature change
  mechanically flows through these call sites, but they still discard
  non-`Throw` completions, matching pre-existing behavior. Not claimed fixed
  here.
- **`for (var {a = yield 1} = {};;)` initializers**: `contains_yield`'s and
  `analyze_statement`'s `Statement::For` arms still only look at a `var`
  init's own `init` expression, not its pattern, matching the equivalent gap
  ADR-2026-09-21-2143 already documents for `await`. Not touched here.
- **Object rest beside a suspending sibling** (`{a = yield 1, ...rest}`):
  unchanged, `pattern_lowering_supported` still declines it regardless of
  suspension kind.
