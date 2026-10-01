# Plan: issue #771 — await/yield in object rest beside a suspending sibling still hangs/replays

## 1. Problem restated

`async function f(){ var {a = await 1, ...rest} = {}; return a; }` (and the
`yield` equivalent in sync/async generators) never reaches the state-machine
lowering #709/#725/#744 built for suspending `var`/`let`/`const` object
binding patterns: `pattern_lowering_supported` (`generator_analysis.rs`)
unconditionally declines any `Pattern::Object` containing an
`ObjectPatternProperty::Rest`, so the whole declaration stays on the
tree-walker's `bind_pattern`/`InlineYield` replay fallback. For `await` this
means the pattern's default blocks synchronously on `await_value` instead of
suspending the async function at the right point relative to the `...rest`
copy. For `yield` it is worse than a missed suspension: per
`docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md`'s
documented residual gap, resuming replays the *entire* object-pattern
binding from the top, which re-invokes every already-consumed property's
getter a second time — if a non-idempotent getter's second call no longer
returns `undefined`, the default is skipped and the value sent to `.next()`
is silently discarded. The fix must lower the `...rest` step itself into the
state machine, which requires `CopyDataProperties` to receive the correct
"already consumed" key-exclusion list even though some of those keys were
themselves produced by a lowered (suspended) computed-key evaluation held in
a `$dstr_key` temp.

## 2. Spec basis

- `sec-runtime-semantics-bindinginitialization` (`BindingPattern :
  ObjectBindingPattern`), specifically the two `ObjectBindingPattern`
  productions ending in `BindingRestProperty`: the exclusion list passed to
  `RestBindingInitialization` is exactly the `boundNames` list returned by
  `PropertyBindingInitialization` over the preceding `BindingPropertyList` —
  i.e. every property key already consumed, in source order, computed
  exactly once.
- `sec-destructuring-binding-patterns-runtime-semantics-propertybindinginitialization`
  (`PropertyBindingInitialization`) — the `PropertyName : BindingElement`
  production evaluates `PropertyName` (the computed key, if any) exactly
  once and returns it as part of the bound-names list that feeds the
  exclusion set.
- `sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization`
  (`RestBindingInitialization`, `BindingRestProperty : ... BindingIdentifier`)
  — `OrdinaryObjectCreate(%Object.prototype%)` then
  `CopyDataProperties(restObj, value, excludedNames)` then
  `InitializeReferencedBinding`. Note the grammar: the rest target is a bare
  `BindingIdentifier`, never a nested pattern — it cannot itself suspend.
- `sec-copydataproperties` (`CopyDataProperties`, §7.3.26 per the existing
  code comment at `src/interpreter/eval/literals.rs:1225`) — excluded keys
  are filtered out of `[[OwnPropertyKeys]]` *before* `[[GetOwnProperty]]`/
  `Get` are called on them. This is the operative correctness requirement:
  an excluded key's `Get` must never be invoked again, which is exactly what
  the replay fallback violates (it re-runs every earlier property's `GetV`,
  including the ones already bound) and what the current `pattern_contains_suspension`
  path trips on if it ever regroups and recomputes a key.
- `sec-runtime-semantics-keyedbindinginitialization` (`KeyedBindingInitialization`,
  `SingleNameBinding`/`BindingElement` productions) — the per-property
  `GetV` + conditional `Initializer` evaluation that `lower_pattern_property`
  already lowers; unchanged by this issue except that its key must now also
  be captured for later reuse.

No new JavaScript syntax or semantics are introduced — this is purely an
engine-internal evaluation-strategy fix (replay-fallback → compiled
state-machine) that must preserve the exact observable behavior the spec
clauses above already mandate.

## 3. Files to touch

Engine:
- `src/interpreter/generator_analysis.rs` — `pattern_lowering_supported`'s
  `ObjectPatternProperty::Rest(_) => false` arm.
- `src/interpreter/generator_transform.rs` — two new `StateTerminator`
  variants (`ObjectRestCopy` and `ToPropertyKey`, see §4 slice 2 for why both
  are needed), `clear_terminator_ic_sites`'s match, `lower_pattern_binding`'s
  `Pattern::Object` arm (accumulator threading), `lower_pattern_property`
  (optional forced key-hoist + emission of the new `ToPropertyKey` terminator
  + return of the key's capture `Expression`), a new
  `lower_object_rest_copy`-style emission helper, and the
  `#[cfg(test)] mod tests` (new positive tests; retire/replace
  `test_unsupported_pattern_shapes_stay_on_the_tree_walker`, which currently
  asserts the exact `{a = await 1, ...rest}` repro stays a 1-state machine).
- `src/interpreter/eval.rs` — async-function driver's `StateTerminator`
  dispatch (new `ObjectRestCopy` and `ToPropertyKey` arms, near the existing
  `ArrayPatternIter` arm at `eval.rs:9460`).
- `src/interpreter/eval/generator_runtime.rs` — sync-generator driver dispatch
  (near `generator_runtime.rs:1897`) and async-generator driver dispatch
  (near `generator_runtime.rs:5316`); both get the same two new arms.
- `src/interpreter/exec.rs` — extract the existing `Pattern::Object`'s
  `ObjectPatternProperty::Rest` body (`exec.rs:1731`–`1746`: build a fresh
  object id, `copy_data_properties`, insert pairs, bind) into a small
  reusable helper (e.g. `bind_object_rest_values`) callable from both the
  unchanged tree-walker call site and the new terminator dispatch, so the
  object-construction/rooting discipline is defined once.

Docs:
- `docs/adr/2026-10-01-HHMM-object-rest-beside-suspending-sibling-lowering.md`
  (new ADR; pick the actual commit-time `HHMM` per the existing
  `docs/adr/` naming convention) — records the `ObjectRestCopy` terminator
  design, the key-capture-accumulator approach, and closes out the "Out of
  scope" item in `docs/adr/2026-09-30-2038-array-pattern-iterator-binding-lowering.md`
  and the matching residual item in
  `docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md` (strike
  through "Object rest beside a suspending sibling" there, same style as
  that file's other "Closed by #NNN" entries).
- `CONTEXT.md:64` — the "destructuring pattern lowering" entry currently
  says "Array patterns and an object rest beside a suspending sibling are
  not lowered yet". Update to drop the object-rest half of that sentence and
  reference the new ADR, and briefly mention the `ObjectRestCopy` terminator
  alongside the existing `$dstr_src`/`$dstr_key`/`$dstr_val` temp vocabulary.

No parser, bytecode compiler, or `ObjectKind`/GC-walker changes: the rest
target's grammar (`BindingRestProperty : ... BindingIdentifier`) is already
parsed correctly (`src/parser/declarations.rs:132`), the bytecode compiler
never sees generator/async-function bodies, and the rest result is an
ordinary plain object — no new `ObjectKind` variant.

## 4. TDD slices

1. **Extract the rest-binding helper, no behavior change.** Pull
   `exec.rs`'s existing `ObjectPatternProperty::Rest` body into
   `bind_object_rest_values(&mut self, source_val: &JsValue, excluded: &[JsPropertyKey]) -> Completion<JsValue>`
   (returns the built rest object as a `JsValue`), called from the unchanged
   tree-walker site. Red/green via the existing suite: run
   `cargo test --release` and the existing `obj-ptrn-rest-*.js` test262
   corpus (directories listed in §5) before and after — identical pass
   counts confirm the extraction is behavior-preserving. No new test file;
   this slice is a prerequisite refactor the rest of the plan depends on,
   justified by needing one shared implementation instead of two divergent
   copies of the CopyDataProperties-building logic.

2. **Flip the support gate and lower the simple (non-computed-key) case.**
   Change `pattern_lowering_supported`'s `Rest` arm to
   `ObjectPatternProperty::Rest(inner) => form == PatternLoweringForm::Declaration && pattern_lowering_supported(inner, form)`
   (recursing defensively, since this lenient parser does not reject a
   non-`BindingIdentifier` rest target the way the grammar implies it
   should — out of scope to fix here, see §7). Add
   `StateTerminator::ObjectRestCopy { source: String, excluded: Vec<Expression>, dest_var: String, next_state: usize }`
   to `generator_transform.rs`, dispatch it in all three driver sites
   (`eval.rs`, both blocks in `eval/generator_runtime.rs`) by evaluating
   `source` and each `excluded` expression, calling
   `self.to_object`/`self.to_property_key`/`bind_object_rest_values` from
   slice 1, and storing the result in `dest_var`. Wire
   `lower_pattern_binding`'s `Pattern::Object` arm to accumulate one
   `Expression` per already-seen property (a literal string for
   `Shorthand`/non-computed `KeyValue` keys — these never need `to_property_key`
   at all, so they are exempt from the double-conversion hazard slice 3
   handles) and, on reaching a trailing `Rest`, emit the `ObjectRestCopy`
   terminator followed by `emit_pattern_binding(kind, rest_pattern.clone(), dest_var, ctx)`
   (mirror `lower_array_pattern_binding`'s own `Drain` → bind sequencing).
   Red: a `generator_transform.rs` unit test asserting
   `async_machine("var {a = await 1, ...rest} = {}; return a;")` produces
   more than one state and exactly one `ObjectRestCopy` terminator (replaces
   `test_unsupported_pattern_shapes_stay_on_the_tree_walker`). Green: the
   implementation above. End-to-end acceptance test:
   `test262-extra/async-function-object-destructuring-rest-after-default.js`
   (mirrors `test262-extra/async-function-array-destructuring-rest-after-await.js`),
   asserting `rest` contains every key except the consumed ones and `a`
   resolves through the awaited default. This slice only has to handle
   `Shorthand` and non-computed `KeyValue` keys ahead of the rest —
   computed keys are slices 3/4.

3. **Computed, non-suspending key before the rest — convert once, reuse
   twice.** `ComputedPropertyName` evaluation performs `ToPropertyKey`
   exactly once per spec, and that same converted key is what both the `GetV`
   read and (if a rest follows) the exclusion list must use —
   `ToPropertyKey` on an object with a custom `toString`/`Symbol.toPrimitive`
   is user-observable, so calling it a second time to rebuild the exclusion
   list (e.g. naively doing `self.to_property_key` again inside
   `ObjectRestCopy`'s own dispatch on the raw key value) is a real bug: it
   would double-invoke that user code. `ToPropertyKey` on an *already*
   primitive String/Symbol is spec-guaranteed side-effect-free (`ToPrimitive`
   passes a primitive through untouched), so the fix is to convert once,
   immediately, and store the *converted* key — any later re-reads of that
   now-primitive value are safe no-ops. Concretely: add a second new
   terminator, `StateTerminator::ToPropertyKey { source: String, dest: String, next_state: usize }`,
   dispatched identically in the three driver sites via
   `self.to_property_key` (propagating a thrown error exactly like `GetV`
   failures already do). `lower_pattern_property` emits this terminator for
   *every* computed key when the enclosing object pattern ends in a `Rest`
   (gated the same way as the forced-hoist below), writing the converted key
   back into the same temp (overwriting the raw value) *before* the GetV
   read, so `pattern_key_read`'s later `source[key_temp]` evaluation finds
   an already-primitive key and its own internal `ToPropertyKey` call is a
   transparent no-op. Also change the fast path (today's
   `!pattern_contains_suspension(&value)` branch, which currently re-embeds
   the raw key expression and relies on a *single* implicit conversion at
   the tree-walker's own read) to always hoist a non-suspending computed key
   to a `$dstr_key` temp first when a trailing `Rest` exists, so there is a
   named temp for the `ToPropertyKey` terminator to convert in place. The
   accumulator records `Identifier(key_temp)` (now holding the converted
   key) for the eventual `ObjectRestCopy`. Red: a counting-probe test using
   an object key with a counting `toString` (e.g.
   `{[{toString(){n++;return 'x'}}]: a, b = await 1, ...rest}`) asserting
   `n === 1` and that `rest` excludes `'x'`, plus a plain call-count test for
   `{[k()]: a, b = await 1, ...rest}` — both as a `generator_transform.rs`
   unit test and
   `test262-extra/async-function-object-destructuring-rest-computed-key-order.js`
   (mirroring `test262-extra/async-function-destructuring-computed-key-await-order.js`'s
   style, extended with the `toString`-counting variant). Green: the
   `ToPropertyKey` terminator plus the forced-hoist change.

4. **Computed, suspending key before the rest.** `{[await k()]: a, ...rest}`
   — the key is already hoisted to a temp holding the *raw* value by the
   existing suspension-driven path (`transform_yielding_expression`, which
   this plan does not change — it stays suspension-infrastructure-generic
   and property-key-agnostic). Slice 3's `ToPropertyKey` terminator still
   runs once, right after that raw hoist completes and before the GetV read,
   converting the same temp in place; the accumulator reuses that one
   `Identifier(key_temp)` rather than hoisting or converting a second time.
   Red: `test262-extra/async-function-object-destructuring-rest-await-key-order.js`,
   using the same counting-`toString`-on-an-awaited-key-expression shape as
   slice 3 (e.g. `{[await {toString(){n++;return 'x'}}]: a, ...rest}`)
   asserting a single call and the correct exclusion. Green: sequencing the
   existing suspension hoist and the new `ToPropertyKey` terminator so the
   conversion happens exactly once, after resume, before the read.

5. **`yield` + the documented non-idempotent-getter bug.** Port the ADR
   2026-09-22-1752 repro into a passing test: a generator with
   `var {a = yield 1, ...rest} = src` where `src`'s relevant property is an
   accessor returning `undefined` on its first call and a different value on
   a (now impossible) second call; assert the sent `.next()` value is used
   for `a` and the getter is called exactly once. Red today (replay would
   silently discard the sent value — confirms the bug before touching
   production code); green once slices 2–4 land, since `yield` already
   shares `pattern_needs_lowering`'s suspension-agnostic gate. New file:
   `test262-extra/generator-yield-object-destructuring-rest-non-idempotent-getter.js`.

6. **Async generator, plus the catch-param/for-of-head `yield` case that
   comes free.** Confirm the third driver site: extend or add alongside
   `test262-extra/async-generator-yield-in-declaration-pattern-default.js`
   an `await`-in-default-plus-rest and a `yield`-in-default-plus-rest case in
   an async generator body, each suspending and resuming correctly.
   Separately — `hoist_suspending_pattern`'s existing gate
   (`!pattern_contains_yield(pattern) && !pattern_needs_await_lowering(pattern)`)
   triggers on `pattern_contains_yield` alone, with no `pattern_lowering_supported`
   check, for *any* yield-containing pattern. Once slice 2 makes
   `Rest` supported for `PatternLoweringForm::Declaration`, a catch
   parameter or for-in/of head pattern containing `yield` next to a `...rest`
   (e.g. `catch ({a = yield 1, ...rest})`, `for (var {a = yield 1, ...rest} of x)`)
   is *already* rewritten by the existing desugar into
   `catch ($tmp) { let {a = yield 1, ...rest} = $tmp; ... }`, and that
   synthesized `let` is an ordinary `Declaration`-form statement — so it
   reaches the new `ObjectRestCopy` lowering automatically, with no
   `ConstrainedDeclaration`-side code change at all. This is a real,
   in-scope fix this issue produces as a side effect, not a coincidence to
   guard against: add positive tests for both shapes (new
   `generator_transform.rs` unit tests plus a
   `test262-extra/generator-yield-catch-param-destructuring-rest.js` /
   `generator-yield-for-of-head-destructuring-rest.js` pair), including the
   non-idempotent-getter check from slice 5 for the catch-param shape, since
   it closes the exact same replay-correctness gap there too.

7. **Nesting.** `{x: {a = await 1, ...rest}}` (object-rest nested in another
   property's value) and `[{a = await 1, ...rest}] = x` (object-rest nested
   in an array element) each get their own accumulator from a fresh
   `lower_pattern_binding` call — add a `generator_transform.rs` unit test
   mirroring `test_array_pattern_nested_in_object_is_lowered` asserting
   exactly one `ObjectRestCopy` terminator per nesting level and correct
   exclusion scoping (no cross-contamination between the outer and inner
   pattern's consumed keys).

8. **Guard the genuinely-still-unsupported forms stay declined.** Unlike
   `yield` (slice 6), an *`await`-only* pattern with a trailing rest at a
   catch-param/for-in/of-head/C-style-for-init site is not touched by
   `hoist_suspending_pattern` (its gate's `pattern_needs_await_lowering`
   half still evaluates `pattern_lowering_supported` under
   `PatternLoweringForm::ConstrainedDeclaration`, where `Rest` stays
   declined) — it must keep running on the pre-existing single
   non-suspending-call binding, unchanged. One regression test
   (`generator_transform.rs` unit test, analogous to the retired test from
   slice 2) asserting an async-function/async-generator
   `catch ({a = await 1, ...rest}) {}` and
   `for (var {a = await 1, ...rest} of x) {}` (no `yield` anywhere in either
   case) still produce a single-state machine, confirming
   `PatternLoweringForm::ConstrainedDeclaration`'s gate is unaffected by this
   change. This is a safety net for §7's out-of-scope boundary, not new
   functionality.

## 5. Test surface

Targeted test262 runs:
- `test262/test/language/statements/variable/`, `.../let/`, `.../const/` —
  3 files each (`obj-ptrn-rest-getter.js`, `obj-ptrn-rest-skip-non-enumerable.js`,
  `obj-ptrn-rest-val-obj.js`; found via `*obj-ptrn-rest*`, excluding the
  unrelated `*ary-ptrn-rest*` array-rest files that a bare `*ptrn-rest*`
  glob would also match) must keep passing unchanged (slice 1's extraction)
  and gate correctly (slices 2+, no suspension in these files so they stay
  on the unchanged non-lowered path).
- `test262/test/language/statements/for-await-of/` — 58 object-rest files
  exercising the declaration-form head binding, which also funnels through
  `bind_object_rest_values` after slice 1; run as a regression net for the
  extraction.
- `test262/test/language/statements/for-of/` (36), `.../for-in/` (2),
  `test262/test/language/expressions/object/` (18),
  `test262/test/language/statements/class/` (72),
  `test262/test/language/expressions/class/` (72),
  `test262/test/language/expressions/assignment/` (27) — broader object-rest
  corpus; none of these exercise suspension, so they are a pure regression
  net for slice 1, not expected to change.
- `test262/test/language/statements/generators/`,
  `test262/test/language/expressions/generators/`,
  `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/` — no existing
  object-rest-plus-suspension coverage was found there, but run them anyway
  as the nearest neighborhood to the new lowering.

New `test262-extra/` files (none of this shape exists in test262 today —
confirmed by searching the whole suite for `await`/`yield` co-occurring with
`...` inside a `var`/`let`/`const` object pattern, following the existing
file-naming and `esid`/`info` commentary conventions used by
`test262-extra/async-function-array-destructuring-rest-after-await.js` and
`test262-extra/generator-yield-in-declaration-pattern-default.js`):
- `async-function-object-destructuring-rest-after-default.js` (slice 2)
- `async-function-object-destructuring-rest-computed-key-order.js` (slice 3)
- `async-function-object-destructuring-rest-await-key-order.js` (slice 4)
- `generator-yield-object-destructuring-rest-non-idempotent-getter.js` (slice 5)
- an async-generator addition/sibling file, plus
  `generator-yield-catch-param-destructuring-rest.js` and
  `generator-yield-for-of-head-destructuring-rest.js` (slice 6)

`cargo test --release` covers the Rust-level unit tests added to
`generator_transform.rs` in slices 2, 3, 7, and 8.
`uv run python scripts/run-custom-tests.py` is not relevant here (no
`tests/` additions planned — this is pure spec-correctness, not an
engine-internal diagnostic or resource-limit concern).

## 6. Regression risk

- **Shared hot path**: `lower_pattern_binding`/`lower_pattern_property` are
  the exact machinery #709/#727/#744 already ship on for every suspending
  object pattern in the currently-passing baseline. Slice 3's forced
  key-hoist must be gated strictly on "this object pattern ends in a
  `Rest`" so the overwhelmingly more common no-rest suspending-object-pattern
  path (already in `test262-pass.txt`) takes exactly the same codegen it
  does today — any broadening of the hoist condition risks extra states /
  extra temp assignments on patterns that currently pass.
  `test_unsupported_pattern_shapes_stay_on_the_tree_walker`'s replacement
  and slice 8's guard test are the direct check for this.
- **GC rooting**: `CopyDataProperties` can run arbitrary user code (a Proxy's
  `getOwnPropertyDescriptor`/`[[Get]]` traps, or an accessor property), which
  can trigger GC before the newly created rest object is reachable from any
  rooted root. Slice 1's extraction is deliberately a pure move (not a
  rewrite) specifically so the new terminator dispatch inherits whatever
  rooting discipline the existing tree-walker Rest arm already has, instead
  of a second, independently-written (and possibly differently-buggy) copy.
- **`for_of_stack`/abrupt-unwind machinery is not reused**: unlike
  `ArrayPatternIter`, `ObjectRestCopy` pushes nothing onto `for_of_stack` —
  `CopyDataProperties` opens no iterator, so a `throw` crossing it needs no
  special unwind registration. Double-check the three driver dispatch sites
  don't accidentally share state with an enclosing `for_of_stack` entry.
- **Double `ToPropertyKey` conversion**: the single highest-risk correctness
  trap in this plan (see §4 slice 3). Any implementation that re-derives the
  exclusion-list key by calling `self.to_property_key` again on the *raw*
  key value inside `ObjectRestCopy`'s own dispatch — instead of reusing a
  key already converted in place by the dedicated `ToPropertyKey` terminator
  — silently double-invokes a computed key's `toString`/`Symbol.toPrimitive`.
  No existing test262 coverage catches this (test262 has no
  object-rest-beside-suspending-computed-key case at all, see §5), so the
  counting-`toString` test262-extra cases in slices 3/4 are the only
  guardrail; do not skip them even though they feel redundant with the
  plainer call-count tests.
- **`hoist_suspending_pattern`'s yield-triggers-regardless-of-support
  behavior is relied on, not just tolerated** (§4 slice 6): if a future
  change makes that gate smarter (e.g. consulting `pattern_lowering_supported`
  before deciding to hoist), it would need to keep triggering for a
  yield-containing `Rest` pattern specifically, or this issue's catch-param/
  for-of-head fix would silently regress back to the replay fallback.
- **Three driver sites must agree**: `eval.rs` (async function) and both
  blocks in `eval/generator_runtime.rs` (sync generator, async generator)
  need the identical new `match` arm; a sync-generator-only or
  async-generator-only fix would leave the other two still declining or,
  worse, panicking on an unhandled `StateTerminator` variant if the gate is
  flipped before all three dispatches exist. Land the terminator definition,
  all three dispatches, and the gate flip in the same slice (slice 2).
- **Bytecode compiler and `ObjectKind` are unaffected** (see §3) — no
  regression surface there.
- **Node-compat library harnesses**: `{a = await 1, ...rest}` is an
  unusual-enough idiom that none of the currently-green libraries
  (`decimal.js`, `big.js`, `acorn`, `zod`, `moment`, `luxon`, etc.) are
  expected to exercise it; this fix can only improve correctness relative to
  the existing replay fallback, not regress it, for any code that does.

## 7. Out of scope

- **`PatternLoweringForm::ConstrainedDeclaration`, `await`-only.** A catch
  parameter, for-in/of head, or C-style for-init pattern with `...rest`
  beside an *awaiting* (no `yield` anywhere) sibling default stays declined,
  exactly as `Pattern::Array` already is for the same form: `EnterCatch`/
  `ForOfHead` bind via a single non-suspending runtime call with no way to
  drive a multi-state terminator sequence, and `pattern_needs_await_lowering`
  checks `pattern_lowering_supported` under `ConstrainedDeclaration`
  specifically to keep this path out. Note this is narrower than it sounds:
  per §4 slice 6, the *`yield`* variant of this same site shape is already
  in scope and fixed by this plan, for free, via `hoist_suspending_pattern`'s
  existing yield-triggered desugar. A future issue could extend that same
  desugar to trigger on `pattern_needs_await_lowering` too (mirroring how
  issue #726 already widened it from yield-only to also cover await for the
  non-rest case) and close the `await`-only gap, but that is a separate,
  independently-sized change not bundled here.
- **Destructuring-*assignment*-form object rest beside a suspending
  sibling** (`({a = await 1, ...rest} = x)`, no `var`/`let`/`const`) —
  `lower_pattern_assignment`/`lower_pattern_assignment_property` get no
  `ObjectRestCopy`-equivalent in this plan; it stays on the tree-walker/replay
  path with the same idempotency-bug class, one layer deeper. Tracked as a
  follow-up, not bundled here.
- **`for (var {a = await 1, ...rest} = x;;)` C-style for-init** — already a
  pre-existing, documented gap (`transform_for_statement`'s `ForInit::Variable`
  branch only lowers when the *init expression* itself suspends, never
  consulting `pattern_needs_lowering`); unrelated to and not touched by this
  plan.
- **The parser's missing early-error for a non-`BindingIdentifier` object
  rest target** (`BindingRestProperty` grammar is `... BindingIdentifier`
  only; this parser currently accepts `{...{a}}` without a SyntaxError) —
  a separate, pre-existing spec-compliance gap, orthogonal to suspension;
  not fixed here. Slice 2's defensive recursive check in
  `pattern_lowering_supported` only prevents this plan's own lowering from
  mishandling such an input, it does not reject it earlier.
- **Formatting/refactor cleanup** beyond the one-time extraction in slice 1:
  no renaming of unrelated `lower_pattern_property`/`lower_pattern_binding`
  parameters, no restructuring of `generator_analysis.rs` beyond the single
  `Rest` arm, no touching the array-pattern (`ArrayPatternIter`) code path
  at all.
