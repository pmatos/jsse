# Object rest beside a suspending sibling: `ObjectRestCopy`/`ToPropertyKey`

Issue #771, split out of #725: `pattern_lowering_supported` declined any
`Pattern::Object` with an `ObjectPatternProperty::Rest`, regardless of
whether a sibling property's default suspended, so `{a = await 1, ...rest}`
(and the `yield` equivalent) stayed on the tree-walker's
`bind_pattern`/InlineYield replay fallback. For `await` this meant the
default blocked synchronously instead of suspending at the right point
relative to the `...rest` copy. For `yield` it was worse: per
ADR-2026-09-22-1752, replaying the whole object-pattern binding on resume
re-invokes every already-consumed property's getter a second time, silently
discarding the value sent to `.next()` if the getter's second call no longer
returns `undefined`.

## Decision: two new terminators, an accumulator in `lower_pattern_binding`

`pattern_lowering_supported`'s `Rest` arm becomes
`form == PatternLoweringForm::Declaration && pattern_lowering_supported(inner, form)`
— lowered only for the unconstrained `Declaration` form, exactly like array
patterns: `EnterCatch`/`ForOfHead` bind via a single non-suspending runtime
call with no way to drive a new terminator, so a suspending object-rest
pattern at those sites still falls back to the tree-walker/replay path (see
"Post-review follow-up" below for the one place this *didn't* need a new
terminator to fix).

Two terminators, both dispatched identically in all three state-machine
drivers (`eval.rs`'s async-function driver, and the sync/async-generator
drivers in `eval/generator_runtime.rs`):

- **`ObjectRestCopy { source, excluded, dest_var, next_state }`** —
  `RestBindingInitialization`'s object-construction step (§14.3.3.3):
  `ToObject(source)`, then `CopyDataProperties` excluding every key named by
  `excluded`, bound into `dest_var`. Runtime half lives in a new
  `Interpreter::object_rest_copy`, built directly on the `bind_object_rest_values`
  helper extracted from the tree-walker's own `Pattern::Object` `Rest` arm
  (a pure move, so the new terminator inherits whatever GC-rooting
  discipline the tree-walker's rest-binding already has, rather than a
  second, independently-written copy).
- **`ToPropertyKey { source, dest, next_state }`** — converts the raw value
  held in `source` to its canonical property key (§7.1.19) and writes the
  converted primitive (string or symbol) back into `dest`, usually the same
  temp. Exists purely to solve the double-conversion hazard below.

`lower_pattern_binding`'s `Pattern::Object` arm now threads an
`excluded: Vec<Expression>` accumulator through the property loop: each
`KeyValue`/`Shorthand` property that precedes a trailing `Rest` contributes
one expression (a string literal for a non-computed key, or the already-converted
computed-key temp — see below), and hitting the `Rest` emits `ObjectRestCopy`
with the accumulated list, then binds the rest pattern from `dest_var`.

## The double-`ToPropertyKey` hazard

A computed key's `ToPropertyKey` conversion is user-observable
(`toString`/`Symbol.toPrimitive`) and must happen exactly once per spec. The
existing non-rest lowering (`lower_pattern_property`) already relies on
this invariant holding *implicitly*: a non-suspending computed key is
re-embedded verbatim in the single non-suspending `GetV` read the
tree-walker performs, which converts it once; a suspending one is hoisted to
a `$dstr_key` temp holding the **raw** value, converted once by that same
`GetV` read after resume. Both are safe as long as nothing else ever reads
the key a second time.

Once a trailing `Rest` needs the same key for its exclusion list, that
assumption breaks: naively re-deriving the excluded key by calling
`to_property_key` again on the raw value inside `ObjectRestCopy`'s own
dispatch — or re-embedding the raw computed-key expression a second time —
would silently double-invoke the key's `toString`/`Symbol.toPrimitive`.

The fix: convert once, explicitly, via the new `ToPropertyKey` terminator,
and store the **already-converted** primitive back into the temp. Re-running
`ToPropertyKey` on an already-primitive string or symbol is spec-guaranteed
side-effect-free (`ToPrimitive` passes a primitive through untouched), so
every later read of that temp — the property's own `GetV` (`pattern_key_read`,
unchanged) and `ObjectRestCopy`'s exclusion-list conversion (inside the new
shared `Interpreter::object_rest_copy` helper) — is a safe no-op, not a
second real conversion. `lower_pattern_property` gained a `capture_key: bool`
parameter (true only when the enclosing pattern ends in `Rest`) that forces
this hoist-and-convert path for a *non-suspending* computed key too (the
existing fast path otherwise skips any temp at all), gated strictly so the
far more common no-rest suspending-object-pattern path takes identical
codegen to before.

Verified with a counting-`toString` key ahead of a trailing rest, both
non-suspending (`{[k]: a, b = await 1, ...rest}`) and suspending
(`{[await k]: a, ...rest}`): the counter reads exactly `1` in both cases, and
`rest` correctly excludes the computed key.

## Post-review follow-up: catch-parameter yield was never actually wired up

Writing the catch-param/for-of-head test262-extra coverage this issue's plan
called for surfaced a separate, pre-existing bug, unrelated to object rest
specifically: `catch ({a = yield 1})` — no rest at all — silently replayed
the whole catch-parameter binding on resume, exhibiting the exact same
non-idempotent-getter discard ADR-2026-09-22-1752 already documented and
accepted as a known limitation for that shape.

That ADR's "Post-review follow-up" section explicitly decided *not* to widen
`contains_yield`/`contains_suspension` for a catch-param-only yield, reasoning
that the existing InlineYield fallback already "suspends and resumes
correctly" there. That reasoning predates `hoist_suspending_pattern`
(introduced later, by #726/#744, gated on `!pattern_contains_yield(pattern)
&& !pattern_needs_await_lowering(pattern)` — i.e. *already* intended to
desugar a yield-containing catch param into a synthesized `let <pattern> =
$tmp;`, which the ordinary `Statement::Variable` lowering then picks up).
But three places never caught up to that intent:

- `analyze_generator_body`'s `Statement::Try` arm called `collect_pattern_vars`
  (bound names only) on the catch parameter, never `analyze_pattern_expressions`
  (the pass that registers a `YieldPoint` for a pattern's defaults/computed
  keys) — so a function whose *only* suspension was a catch-param default
  never got a single `YieldPoint` registered, and
  `analysis.yield_points.is_empty()` stayed true: the whole function took
  the single-state "simple machine" shortcut, the synthesized `let` was
  never reached by anything but the tree-walker, and `hoist_suspending_pattern`'s
  later work was moot.
- `contains_yield`'s and `contains_suspension`'s `Statement::Try` arms
  checked only the catch body, never `h.param`'s pattern — unlike the
  `ForIn`/`ForOf` arms a few lines above, which already check both
  `for_in_of_head_contains_yield` (ungated — any yield forces the compiled
  machine) and `for_in_of_variable_head_contains_await` (shape-gated via
  `pattern_needs_await_lowering`, so an unsupported shape stays fully
  inline). Catch params needed the exact same pair; they had neither.

**Fix:** mirror the `ForIn`/`ForOf` precedent exactly — add
`analyze_pattern_expressions(param, analysis, ctx)` alongside
`collect_pattern_vars` in `analyze_generator_body`'s catch-param handling,
and add `h.param.as_ref().is_some_and(pattern_contains_yield)` /
`h.param.as_ref().is_some_and(|p| pattern_contains_yield(p) ||
pattern_needs_await_lowering(p))` to `contains_yield`'s and
`contains_suspension`'s `Statement::Try` arms respectively. This closes the
gap for *every* yield-in-catch-param shape, not just the object-rest one
this issue is about — confirmed via the same non-idempotent-getter probe
with no trailing rest at all (`calls === 1`, sent value used), and via the
full test262 suite (28,277 scenarios across the try/destructuring/generator/
async corpora) showing zero regressions.

This also means the "a catch parameter or for-in/of-head pattern containing
`yield` next to a `...rest` ... is already rewritten by the existing desugar
... and reaches the new `ObjectRestCopy` lowering automatically" claim this
issue's implementation plan made (citing #726's yield-triggered desugar) was
only half true: the desugar machinery existed, but nothing upstream of it
ever triggered for the pure-yield-no-other-suspension case. Both halves
needed fixing together.

## Updates to prior ADRs

- ADR-2026-09-30-2038's "Out of scope" item ("Object rest beside an
  awaiting/yielding sibling ... tracked as a follow-up (issue #771)") is
  closed by this change.
- ADR-2026-09-22-1752's "What this still does not cover" item ("Object rest
  beside a suspending sibling ... not attempted here") is closed by this
  change. Its "Post-review follow-up" section's claim that catch-param/
  for-in/of-head yield needed no detection widening is superseded by the
  fix above.

## Out of scope (unchanged from the plan)

- **`PatternLoweringForm::ConstrainedDeclaration`, `await`-only**: a catch
  parameter, for-in/of-head, or C-style for-init pattern with `...rest`
  beside an *awaiting* (no `yield` anywhere) sibling default stays declined,
  same as array patterns at those sites — `EnterCatch`/`ForOfHead` have no
  way to drive a multi-state terminator sequence. The `yield` variant of the
  same site shape is in scope and fixed (see above).
- **Destructuring-*assignment*-form object rest beside a suspending
  sibling** (`({a = await 1, ...rest} = x)`) — `lower_pattern_assignment`
  gets no `ObjectRestCopy`-equivalent here; stays on the tree-walker/replay
  path with the same idempotency-bug class, one layer deeper.
- **`for (var {a = await 1, ...rest} = x;;)` C-style for-init** — a
  pre-existing, documented gap (`transform_for_statement`'s `ForInit::Variable`
  branch only lowers when the init *expression* itself suspends), unrelated
  to this change.
- **The parser's missing early-error for a non-`BindingIdentifier` object
  rest target** — a separate, pre-existing spec-compliance gap, orthogonal
  to suspension.
