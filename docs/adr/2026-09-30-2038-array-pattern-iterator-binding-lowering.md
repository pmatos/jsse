# Lowering array binding-pattern iterator stepping into suspension states

Issue #725, a follow-up of #709 (object-pattern default lowering) and #727/#744
(the equivalent gap for `yield`). `pattern_lowering_supported` declined every
`Pattern::Array`, so a `var`/`let`/`const` array binding pattern whose default
contains an `await` or `yield` stayed on the tree-walker's `bind_pattern`,
which evaluates the default through plain `eval_expr` — blocking on
`await_value` (draining the microtask queue inline instead of suspending the
async function) or silently discarding a `Completion::Yield` from a sync/async
generator (per #744's fix, this specific discard was already patched, but the
array pattern still couldn't reach the state machine's suspension states at
all — see "What #744 already fixed" below).

## Decision

Extend the #709 lowering machinery to array patterns:

- `pattern_lowering_supported` (`generator_analysis.rs`) gains a
  `Pattern::Array` arm: supported when every element and rest sub-pattern is
  itself supported (elisions trivially are). This flips `pattern_needs_lowering`
  to `true` for a suspending array pattern — both `await` and `yield`, since
  `pattern_needs_lowering` already gates on `pattern_contains_suspension`, not
  `pattern_contains_await` specifically (that widening predates this change,
  from #744).
- A new `StateTerminator::ArrayPatternIter { op, iter_var, next_state }`
  (`generator_transform.rs`) performs one `IteratorBindingInitialization`
  primitive per state:
  - `Init { iterable }` — `GetIterator(iterable, sync)`, storing the iterator
    in a fresh temp and pushing a `ForOfLoopState` onto the driver's
    `for_of_stack`.
  - `Step { dest_var: Option<String> }` — one `IteratorStepValue`: `None` is an
    elision (step and discard); `Some` stores the stepped value (or `undefined`
    once the iterator is observed done) into the temp.
  - `Drain { dest_var: String }` — `IteratorStepValue` looped to exhaustion for
    a `BindingRestElement`, collecting into a fresh array.
  - `Finish` — normal-completion `IteratorClose` when the pattern
    under-consumed the iterator (`sec-runtime-semantics-bindinginitialization`'s
    "If iteratorRecord.[[Done]] is false, return ? IteratorClose(...)" is a
    normal-completion step, not only an abrupt-exit one). A no-op once
    `Step`/`Drain` has already observed `[[Done]]`.

  This is always synchronous — `GetIterator` for a binding pattern is never
  `for await`, even inside an async function — so unlike `Await`, this
  terminator never itself suspends; only an element's own default (lowered
  separately, reusing `lower_conditional_default`/`transform_yielding_expression`
  exactly as the object-pattern case does) can.

- `lower_array_pattern_binding` (`generator_transform.rs`) emits one `Step`
  per element regardless of whether that element suspends — `IteratorStepValue`
  order is itself observable (an instrumented iterable's `next()` must fire
  once per element, in order, with no pre-stepping past a suspended default) —
  but only recurses into `lower_pattern_binding`/`lower_conditional_default`
  for an element whose own sub-pattern needs further suspension-lowering.
  Everything else binds through the tree-walker in one call from the
  already-stepped temp, mirroring how `lower_pattern_property`'s own
  non-suspending branch batches a whole property into one statement.
- `lower_pattern_binding`'s top-level dispatch changed from an
  `Object`-only `let ... else` to a `match` over `Object` / `Array` / other,
  so both the top-level declarator and `lower_pattern_property`'s recursive
  call route an awaiting/yielding array pattern into
  `lower_array_pattern_binding` — this is what makes `{x: [a = await 1]}`
  (an array pattern nested inside an object property) fall out for free,
  without any array-pattern-in-object special case.
- The `ForOfLoopState` pushed at `Init` reuses the driver's existing
  abrupt-unwind path (`unwind_for_of!` in the async-function driver,
  `route_generator_exception`/`discard_failed_generator_for_of_loop` in the
  sync/async-generator drivers) so a `throw` crossing a still-open array
  pattern — the default's own awaited/yielded value rejecting, or a later
  statement in an enclosing `try`/`finally` — closes the iterator exactly
  once, for free. `Step`/`Drain` pop their own entry from `for_of_stack` the
  instant they observe `[[Done]]` (whether by exhaustion or by
  `IteratorStep`/`IteratorValue` failing, which `sec-iteratorstepvalue` says
  sets `[[Done]] = true` *before* the failure propagates) rather than leaving
  the pop to `Finish` alone — a `...rest` drain always ends done-by-construction
  and so never reaches `Finish` at all; without this, its entry would leak on
  `for_of_stack` for the rest of the function's execution, corrupting a later,
  unrelated `break`/`continue` depth computation across a real loop.

Three driver sites needed the same four-op `match` arm: the async-function
driver (`eval.rs`), and both the sync-generator and async-generator drivers
(`eval/generator_runtime.rs`) — a suspending array pattern is reachable from
all three (an `await` only from async function/async generator bodies; a
`yield` from any of the three, sync generators included).

## What #744 already fixed, and what it left open

#744 fixed `bind_pattern`'s tree-walking evaluation of an array pattern's
default to stop swallowing `Completion::Yield` (three catch-all
`_ => JsValue::UNDEFINED` arms replaced with real completion propagation), so
a `yield` in an array-pattern default at least suspends the generator via the
existing `InlineYield` replay fallback instead of being silently discarded.
That fix's own ADR (`2026-09-22-1752-yield-in-declaration-pattern-default.md`)
documents the fallback's known limitation directly: replaying the whole
statement from the top on resume re-evaluates the RHS and re-opens a fresh
iterator, confirmed via a side-effect probe (`var [a = yield 1] = (n++, [])`
resumes with `n === 2`). This ADR's lowering removes that replay path
entirely for array patterns that reach it — the iterator is opened once, in
`Init`, and the `for_of_stack` entry (not a re-run of the source statement)
is what survives a suspension.

## Iterator-order correctness, verified

Per `sec-runtime-semantics-iteratorbindinginitialization`, `IteratorStepValue`
is called exactly once per element in source order, and the awaited/yielded
default runs strictly between two steps — never before both, never
pre-stepping ahead. Verified with an instrumented iterable
(`async-function-array-destructuring-iterator-step-order.js`) that a second
element's step happens only after the first element's suspended default
resumes.

## Regression risk callouts

- **Non-suspending array patterns are untouched.** `pattern_needs_lowering`
  still requires `pattern_contains_suspension`; a plain `var [a, b] = arr`
  short-circuits to the unchanged single-state path.
~~**`for (var [a = await 1] = x;;)` initializers stay out of scope**, same as
the pre-existing object-pattern gap: `transform_for_statement`'s
`ForInit::Variable` branch only lowers when the *init expression* itself
suspends, never consulting `pattern_needs_lowering`.~~
**Closed by #773** (`transform_for_statement`'s `ForInit::Variable` branch now
also consults `pattern_needs_await_lowering`, routing through
`transform_variable_declaration`) **for object patterns, then by #774** for
array patterns (`pattern_needs_await_lowering`'s shape gate widened to admit
`Pattern::Array`).

~~**`for-of`/`for-in` heads and `catch` params are untouched** — both bind
through `ForInOfLeft`/`EnterCatch` directly, never through
`transform_variable_declaration`. Issue #726 tracks the equivalent
`await`-in-catch-param/for-head-default gap.~~
**Closed by #773** for object patterns (`hoist_suspending_pattern` re-homes
the pattern into a synthesized declaration that *does* route through
`transform_variable_declaration`) **and by #774** for array patterns, via the
same hoist, once `pattern_needs_await_lowering`'s shape gate stopped
excluding `Pattern::Array`.
- **Bytecode compiler has no exposure** — it never sees generator/async-function
  state machines.

## Out of scope

~~**Object rest beside an awaiting/yielding sibling** (`{a = await 1, ...rest}`)
remains unsupported: `pattern_lowering_supported`'s
`ObjectPatternProperty::Rest => false` arm is untouched by this change. It
needs `CopyDataProperties` with a consumed-key exclusion list (some excluded
keys may themselves live in `$dstr_key` temps from a lowered computed key) —
a different problem shape than array-iterator stepping, shares no machinery
with this ADR's change, and is tracked as a follow-up (issue #771).~~
**Closed by #771**: see
`docs/adr/2026-10-01-0233-object-rest-beside-suspending-sibling-lowering.md`.
