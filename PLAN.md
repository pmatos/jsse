# Plan: issue #725 — lower `await` in array binding-pattern defaults

## 1. Problem restated

`generator_transform.rs`'s state-machine lowering (landed for object patterns in
#709 / ADR `2026-09-21-2143-destructuring-pattern-lowering.md`) only recognizes
`Pattern::Object` as a lowerable shape. A `var`/`let`/`const` declarator whose
pattern is `Pattern::Array` — `var [a = await 1] = []`, or an array pattern
nested inside an object pattern's property (`{x: [a = await 1]}`) — is still
emitted as one intact statement into a state body. The tree-walker's
`bind_pattern` then evaluates the default through `eval_expr`, whose `await`
handling (`await_value`) blocks and drains the microtask queue inline instead
of suspending the async function back to its caller. Observable effect: jobs
scheduled before the `await` run in the wrong order relative to the function's
synchronous continuation (`w1,a1,sync-end,…` instead of `sync-end,w1,a1,…`).

This plan closes the array-pattern half of #725: `var`/`let`/`const` array
binding patterns, at top level and nested inside an object pattern's property,
lower their `await`-containing elements into suspension states the same way
#709 lowered object patterns. The other manifestation named in the issue body
— `{a = await 1, ...rest}`, an object *rest* sibling to an awaiting property —
shares no machinery with array-iterator stepping and is explicitly deferred
(see §7).

## 2. Spec basis

- **`sec-runtime-semantics-bindinginitialization`**, clause `BindingPattern :
  ArrayBindingPattern` (spec.html:9647-9653): `GetIterator(value, ~sync~)`,
  then `IteratorBindingInitialization`, then **`If iteratorRecord.[[Done]] is
  false, return ? IteratorClose(iteratorRecord, result)`** — closing the
  iterator when it isn't exhausted is a *normal-completion* step, not only an
  abrupt-exit one. `var [a] = [1, 2, 3]` must call the iterable's `.return()`
  even though nothing throws.
- **`sec-runtime-semantics-iteratorbindinginitialization`** (spec.html:9699-9804):
  - `SingleNameBinding : BindingIdentifier Initializer?` and `BindingElement :
    BindingPattern Initializer?` — if the iterator isn't done, call
    `IteratorStepValue` **exactly once**; the `Initializer` (where the `await`
    lives) runs only when the stepped value is `undefined` (elision or
    exhaustion), and a `BindingElement` whose target is itself a `BindingPattern`
    recurses via ordinary `BindingInitialization` on the one stepped value —
    a plain-value bind, not a continuation of the outer iterator. This is why a
    nested array pattern (`[a, [b = await 1]]`) opens its *own* fresh iterator
    on the value stepped from the outer one, rather than sharing state.
  - `BindingElisionElement : Elision BindingElement` —
    `IteratorDestructuringAssignmentEvaluation` of an `Elision` steps and
    discards, once per hole.
  - `BindingRestElement : ... BindingIdentifier | ... BindingPattern` — loops
    `IteratorStepValue` until `~done~`, collecting into a fresh array; no
    `Initializer` exists on a rest element, so this loop itself never contains
    an `await`.
- **`sec-iteratorstepvalue`** (spec.html:7141-7160): wraps `IteratorStep` +
  `IteratorValue`; if `IteratorValue` throws, `iteratorRecord.[[Done]]` is set
  `true` *before* the completion propagates — the failing step is never
  followed by an `IteratorClose` (the top-level check above already sees
  `[[Done]] = true` and skips it).
- Prior art / conventions this plan follows: ADR
  `docs/adr/2026-09-21-2143-destructuring-pattern-lowering.md` (the #709
  object-pattern lowering — same `pattern_needs_lowering` /
  `pattern_lowering_supported` gate, same "only `await` triggers lowering,
  `yield`-only patterns keep the replay path" rule, same conditional-default
  shape via `ConditionalGoto` on `typeof $v === "undefined"`).

## 3. Files to touch

- `src/interpreter/generator_analysis.rs` — extend `pattern_lowering_supported`
  with a `Pattern::Array` arm (every element/rest sub-pattern must itself be
  `pattern_lowering_supported`; elisions are trivially supported). No change
  needed to `pattern_contains_await` / `pattern_contains_suspension` /
  `pattern_any_expr` — their existing `Pattern::Array` arm already looks
  through elements (see the `#709` diff, `pattern_await_is_found_in_defaults_keys_and_nested_patterns`
  test already exercises `var [a = await 1] = [];`). Update the existing unit
  test `only_object_patterns_are_lowered` (currently asserts
  `!pattern_needs_lowering` for `var [a = await 1] = [];`) to assert `true`
  once this slice lands.
- `src/interpreter/generator_transform.rs`:
  - A new `StateTerminator` variant for the iterator-stepping primitives (GetIterator
    is `sync` only for a binding pattern — never `for await` — so this never
    needs `is_await`/dispose-resources support the way `ForOfHead` does; do not
    route through `ForOfInit`/`ForOfHead`, which carry for-in/dispose baggage
    this doesn't need). Suggested shape, one variant with an op payload so the
    three drivers each need one new `match` arm rather than four:
    ```rust
    ArrayPatternIter {
        op: ArrayPatternIterOp,
        iter_var: String,
        next_state: usize,
    }
    enum ArrayPatternIterOp {
        Init { iterable: Expression },
        Step { dest_var: Option<String> },  // None = elision: step and discard
        Drain { dest_var: String },          // rest: loop-to-exhaustion, no await inside
        Finish,
    }
    ```
    (`Finish` replaces an earlier `CloseIfNotDone` name from draft review —
    same position in the pattern, see below for why it must do more than a
    conditional close.)
    `Init` performs `GetIterator(iterable, sync)` (reuse the same helper the
    tree-walker's `bind_pattern` already calls — `get_iterator`/
    `for_of_init_iterator`), stores the iterator record under a fresh
    `ctx.new_temp_var("dstr_iter")`, and pushes a `ForOfLoopState` onto the
    driver's existing `for_of_stack` (reusing the type as-is, not a new
    parallel struct) purely so the existing abrupt-unwind path
    (`unwind_for_of!` in eval.rs, and its generator-driver equivalents) closes
    it for free on a `throw`/`return` that crosses this point — mirrored on
    `self.gc_root_value(&iterator)` the way `ForOfInit` already does, so no
    `gc.rs` changes are needed (`collect_for_of_stack_roots` already walks
    `for_of_stack` entries' envs; the iterator value's rooting comes from
    `gc_root_value`, generically). `[[Done]]` must survive suspension across an
    `Await` state, so store it as a private function-env temp (e.g.
    `{iter_var}__done`), mirroring the existing `{iter_var}__await` convention
    at `eval.rs:9389` — do **not** reuse `gc_temp_roots` the way the
    tree-walker's `bind_pattern` does (`exec.rs:1451`); that stack is a
    call-scoped Rust local and does not survive a suspend/resume round trip.
    `Step` and `Drain` are synchronous ops (`IteratorStepValue` is never
    awaited for a *sync* binding-pattern iterator, even inside an async
    function — only `for await` uses async iterators); they still need to be a
    terminator (not an ordinary statement) only because they call
    Rust-only helpers (`iterator_step`/`iterator_value`) that the general
    `Statement`/`Expression` evaluators can't reach — there is no existing
    internal-only `Statement`/`Expression` hatch (confirmed: no `Internal`/
    `intrinsic` variant in `src/ast.rs`), so adding one here would be a wider
    diff (parser-adjacent exhaustive matches, IC-site clearing, the pretty
    printer) than one more `StateTerminator` arm, which only touches
    `clear_terminator_ic_sites` and the three drivers already listed below —
    the same shape `ForOfInit`/`ForOfHead` chose for exactly this reason.

    **The `for_of_stack` entry's lifetime must track `[[Done]]`, not just
    "pushed at `Init`, popped at `Finish`."** `StateTerminator::ForOfHead`'s own
    existing done-handling is the precedent to copy exactly (`eval.rs`, the
    `if done { ...; for_of_stack.remove(loop_pos); }` branch a few lines after
    the arm cited above): the moment a `Step` or `Drain` observes
    `IteratorStepValue`'s result as done (or `IteratorStepValue` itself fails,
    which the spec — §2's `sec-iteratorstepvalue` citation — has already set
    `[[Done]] = true` for before the failure propagates), that call must
    locate this pattern's own entry in `for_of_stack` by `iter_var` (`rposition`,
    same as `ForOfHead`) and remove it immediately, in the same step that
    records `{iter_var}__done = true`. Skipping this leaves a stale entry on
    the stack in two observable ways: (a) a pattern ending in `...rest` never
    reaches `Finish` at all (see below), so without an explicit pop inside
    `Drain` its entry leaks for the rest of the function's lifetime, corrupting
    every later `break`/`continue`/`throw` depth computation the §6 risk below
    already flags; (b) `var [a, b = await Promise.reject()] = [1]` — iterator
    exhausted after stepping `a`, then `b`'s default rejects — would otherwise
    have `unwind_for_of!` find the entry still on the stack and call
    `.return()` on an already-exhausted iterator, which `sec-iteratorclose`
    forbids calling at all once `[[Done]]` is true qualifies as done. `Drain`
    always ends with `[[Done]] = true` by construction (it loops
    `IteratorStepValue` to exhaustion), so it always performs this pop itself
    and a pattern ending in a rest element never reaches `Finish`.
    `Finish` is emitted exactly once, at the end of a pattern *without* a
    trailing rest, and runs on the fully-normal path too (see §2's
    `BindingPattern : ArrayBindingPattern` clause — this is the one place this
    plan diverges from "abrupt exits reuse `for_of_stack`" per the issue body).
    Its job: if this pattern's entry is still on `for_of_stack` (i.e. `Step`
    never observed done — the pattern under-consumed the iterator), pop it
    first, then run the same normal-completion `IteratorClose` the driver
    already has for a `break`/`return` crossing a real `for-of` loop
    (`close_for_of_loop` with `Completion::Normal(...)`, *not* the
    `unwind_for_of!` macro — that macro is for completions that are already
    abrupt and is reached separately, per the abrupt-exit slice below). If the
    entry is already gone (pattern fully drained or hit done during its last
    `Step`), `Finish` is a no-op. Get the "pop, *then* close" ordering from a
    real test (slice 6), not by inspection — reversing the order re-closes an
    already-popped/already-done iterator.
  - `clear_terminator_ic_sites` — add the new variant (`Init`'s `iterable`
    needs `clear_expr_ic_sites`; the rest carry no cleared expressions).
  - A new `lower_array_pattern_binding(kind, elements, source, ctx)` alongside
    the existing `lower_pattern_binding`/`lower_pattern_property`, following
    the same per-element "only break up what reaches a suspension" rule: an
    element whose sub-pattern doesn't contain a suspension is still bound
    through one `Step` into a temp, then **`emit_pattern_binding(kind,
    element_pattern, step_tmp, ctx)` directly on the element's own
    (sub-)pattern** — *not* `<kind> [<elem>] = $tmp`. `step_tmp` already holds
    the single value `IteratorStepValue` produced for this element (or
    `undefined` past exhaustion); re-wrapping it as `[<elem>]` would make the
    tree-walker call `GetIterator` on `step_tmp` itself and open a *second*,
    spurious iterator on a bare value instead of binding the element against
    it directly. This mirrors how `lower_pattern_property`'s own
    non-suspending branch (`generator_transform.rs:1850-1857`) binds a whole
    property in one call rather than pre-extracting and re-wrapping — the
    array case differs only in that the value is already a plain temp
    (`step_tmp`, from `Step`) rather than something `emit_pattern_binding`
    re-derives via a property read, since a destructuring source has no
    stable "key" to re-read the way an object property does. Naming, TDZ, and
    anonymous-function-naming for the element's own sub-pattern stay the
    tree-walker's, exactly as for the object case. Only elements that do reach
    a suspension get the full `Step` → `ConditionalGoto` → default-states →
    recurse-into-inner-pattern treatment (the same shape `lower_pattern_property`
    already uses for object defaults) — and that recursion, too, must bind the
    inner target from the already-stepped temp via `emit_pattern_binding`
    (or, when the target itself contains a further suspension, via a nested
    `lower_pattern_binding`/`lower_array_pattern_binding` call keyed to that
    temp), never by re-wrapping the temp in another pattern shape.
  - **`lower_pattern_binding`'s top-level dispatch must change.** Today it is
    `let Pattern::Object(props) = pattern.clone() else { emit_pattern_binding(...); return; }`
    — any non-`Object` pattern (including `Pattern::Array`) falls straight to
    a plain tree-walked sub-statement *regardless of whether it contains a
    suspension*. This is exactly why `{x: [a = await 1]}` is broken today: the
    object property's value recurses into `lower_pattern_binding` on the inner
    array pattern, hits the `else` arm, and emits the intact (still-blocking)
    `<kind> [a = await 1] = $value_temp`. Change the dispatch to a `match` over
    `Pattern::Object` / `Pattern::Array` / everything else, so both entry
    points — the top-level declarator and the recursive call from
    `lower_pattern_property` — route an awaiting array pattern into
    `lower_array_pattern_binding`. This is what makes the nested-in-object case
    fall out for free once the array primitive exists, per the ADR's own note.
  - Update the existing unit test `test_unsupported_pattern_shapes_stay_on_the_tree_walker`:
    remove its `"var [a = await 1] = [];"` and `"var { x: [a = await 1] } = {};"`
    cases (both become lowered), keep `"var { a = await 1, ...rest } = {};"`
    (still unsupported — object rest is out of scope here). Add new positive
    tests mirroring the object-pattern ones (`test_awaiting_default_lowers_to_conditional_await_state`,
    `test_present_property_pattern_without_await_takes_simple_machine`,
    `test_awaiting_computed_key_is_lowered_at_its_own_position`) for: a
    single awaiting element, a present element short-circuiting the default,
    elision before an awaiting element, a rest element following awaiting
    elements, and the nested-in-object case.
- `src/interpreter/eval.rs` — async-function driver: one new `match` arm for
  `StateTerminator::ArrayPatternIter` (near the existing `ForOfInit`/`ForOfHead`
  arms around line 9275-9440), implementing `Init`/`Step`/`Drain`/`Finish`
  against `for_of_stack` as described above. This is the only driver that can
  actually receive this terminator in an *async function* body.
- `src/interpreter/eval/generator_runtime.rs` — async-generator driver (the
  arm near line 4603-4834): same new `match` arm; an `await` inside an array
  pattern default is reachable there too (`async function* g() { var [a =
  await 1] = []; }`). The **sync**-generator driver arm (near line 1555-1612)
  cannot ever receive this terminator — `pattern_needs_lowering` requires a raw
  `Expression::Await`, which is a syntax error outside an async context — but
  verify whether the shared `StateTerminator` match in that driver is written
  exhaustively (as it likely is for `StateTerminator::Await`, which is equally
  unreachable there) and, if so, add the same defensive `unreachable!()`/
  `debug_assert!(false)` arm rather than leaving a compile error.
- `test262-extra/` — new files, see §5.
- `docs/adr/` — new ADR (e.g. `2026-09-22-array-pattern-iterator-binding-lowering.md`)
  recording this decision and referencing `2026-09-21-2143-destructuring-pattern-lowering.md`;
  the prior ADR is left unedited (immutable historical record) and the new one
  narrows its "what this change does not cover" list for the array-pattern
  and nested-array-in-object cases, explicitly carrying forward the object-rest
  gap as still open.

## 4. TDD slices

1. **Gate extension, red via existing test.** Extend `pattern_lowering_supported`
   (`generator_analysis.rs`) with the `Pattern::Array` arm. Update
   `only_object_patterns_are_lowered` to assert `pattern_needs_lowering` is now
   `true` for `var [a = await 1] = [];` and still `false` for
   `var { a = await 1, ...r } = {};`. (This alone does not yet change runtime
   behavior — `lower_pattern_binding`'s dispatch still routes `Pattern::Array`
   through the unchanged `else` arm — so no `sm.states.len()` assertions move
   yet; that's slice 3.)
2. **Terminator + driver plumbing, no callers yet.** Add
   `StateTerminator::ArrayPatternIter`/`ArrayPatternIterOp`, the
   `clear_terminator_ic_sites` arm, and the driver `match` arms in `eval.rs`
   and the async-generator arm of `generator_runtime.rs` (plus the sync-generator
   defensive arm if the match there is exhaustive). Nothing emits this
   terminator yet, so this slice is verified by `cargo build` staying green
   (exhaustive-match compiles) and existing tests staying green — genuinely
   red only in the sense that the code doesn't exist yet; there's no
   behavioral test to fail first here, which is why it's kept as its own small
   commit rather than folded into slice 3's test-driven work.
3. **Single awaiting element, top-level array pattern.** Add
   `lower_array_pattern_binding` and change `lower_pattern_binding`'s dispatch
   to a `match`. Red: a new `generator_transform.rs` unit test analogous to
   `test_awaiting_default_lowers_to_conditional_await_state` —
   `async_machine("var [a = await 1] = []; return a;")` — currently asserts
   `sm.states.len() == 1` (see slice-adjacent removal in
   `test_unsupported_pattern_shapes_stay_on_the_tree_walker`); green once the
   new test instead asserts one `Await` terminator, one `ConditionalGoto`, and
   an `ArrayPatternIter { op: Init, .. }` / `Step` pair preceding them, matching
   the object-pattern precedent's assertion style.
4. **Present element short-circuits the default; elision skips a step.** Red:
   `async_machine("var [a = 5, , b = await 1] = []; return a;")`-shaped test
   asserting the machine still has exactly one `Await` (for `b`'s default) and
   that `a`'s `Step` result is used directly without a `ConditionalGoto` branch
   reaching an `Await`. Green from the same `lower_array_pattern_binding` doing
   per-element "only lower what reaches a suspension."
5. **Iterator-step ordering is observable, real execution.** This is the
   behavioral heart of the issue. Red: a `tests/` or `test262-extra/` case
   (see §5) equivalent to the issue's repro
   (`async function f(){ var [a = await 1] = []; L('a'+a) } f(); L('sync-end');`)
   run through the real interpreter (`cargo test --release` picking up a
   `tests/` harness case, or the custom-test runner) currently prints
   `w1,a1,sync-end,w2`; green once it prints `sync-end,w1,a1,w2`. Also assert
   the two-step ordering case from the issue —
   `async function f(){ var [a = await 1, b = 2] = [undefined, 2]; }` —
   against an iterable whose `next()` is instrumented (a custom iterable
   object logging each call) to prove the *second* element's step happens
   *after* the first element's `await` resumes, not before it (no
   pre-stepping).
6. **Normal-completion `IteratorClose` when not fully drained.** Red: a test
   using a custom iterable whose `return()` method is observable (push to a
   log array), asserting `.return()` **is** called for `var [a = await 1] =
   customIterable` when `customIterable` yields more than one value (default
   not taken) and the array pattern only consumes one, and is **not** called
   when the pattern ends in a rest element or fully drains the iterator. This
   is the `Finish` terminator; per §2, get this from a real green test,
   not by inspection, since it's easy to build only the abrupt-path close and
   silently miss the normal-completion path. Also assert the two negative
   cases that must *skip* `Finish`'s close because `[[Done]]` already flipped
   before `Finish` runs, mirroring the sync precedents
   `test262/test/language/statements/variable/dstr/ary-ptrn-elem-id-iter-step-err.js`
   and `ary-ptrn-elem-id-iter-val-err.js`: a `next()` call that throws, and a
   `next()` result whose `.value` getter throws, each with a following
   awaiting element (`var [a = await 1, b = await 2] = it` where `it`'s second
   `next()`/`.value` throws before `b`'s `Step`) — in both, `.return()` must
   **not** be called, since §2's `sec-iteratorstepvalue` citation already set
   `[[Done]] = true` before the throw reached this code, so the `Step`/`Drain`
   pop (see §3) must have already removed the entry by the time the throw
   propagates to `Finish` or to an enclosing `unwind_for_of!`.
7. **Abrupt exit closes exactly once.** Red: a case where the default's
   `await`ed promise rejects (`var [a = await Promise.reject(new Error())] =
   customIterable`) or the pattern is inside a `try`/`finally` that itself
   throws afterward, asserting the iterable's `.return()` fires exactly once
   (not zero, not twice) via the existing `for_of_stack` unwind path.
8. **Rest element.** Red/green pair for `var [a, ...rest] = [1, 2, 3]` and
   `var [a = await 1, ...rest] = it` (rest itself never contains an `await`,
   but a preceding element does, so the pattern is still lowered) — asserts
   `rest` collects the remaining values and no `Finish` fires (spec:
   `[[Done]]` is already `true` after a rest drain).
9. **Nested array-in-object composes for free.** Red:
   `async_machine("var { x: [a = await 1] } = {x: []}; return a;")` currently
   emits the intact statement (`sm.states.len() == 1`, the bug this issue
   reports for the nested case); green once `lower_pattern_binding`'s dispatch
   change (slice 3) makes the property's recursive call route into
   `lower_array_pattern_binding`. No new production code beyond slice 3 should
   be needed here — if it is, that's a sign the dispatch change missed a case.
10. **Async generator parity.** Red/green pair mirroring slice 5's ordering
    test but inside `async function* g() { var [a = await 1] = []; yield a; }`,
    exercised through the async-generator driver arm added in slice 2.

## 5. Test surface

- **Regression set (must stay green, exercises the tree-walker, not the new
  path):** `test262/test/language/statements/variable/dstr`,
  `test262/test/language/statements/let/dstr`,
  `test262/test/language/statements/const/dstr` (57 `ary-ptrn-*` files each —
  confirmed by listing), plus `test262/test/language/statements/async-generator/dstr`
  and `test262/test/language/expressions/async-generator/dstr`. Run targeted:
  `uv run python scripts/run-test262.py test262/test/language/statements/variable/dstr/`
  (and siblings). These files run patterns at top level / in sync contexts and
  don't test microtask ordering — test262 has no ordering-sensitive test for
  this (confirmed: the only `await`-named array-pattern file under
  `statements/variable/dstr`, `ary-ptrn-elem-id-static-init-await-*.js`, tests
  `await` as an identifier in a class static block, unrelated to this bug),
  which is exactly why #709 needed `test262-extra/` coverage and this does too.
- **New coverage, `test262-extra/`** (spec-correct behavior test262 doesn't
  reach, following the `async-function-destructuring-*` naming from #709):
  - `async-function-array-destructuring-default-await-suspends.js` — the
    issue's own repro, asserting job order.
  - `async-function-array-destructuring-default-not-evaluated-when-present.js`
    — present/elision elements don't tick.
  - `async-function-array-destructuring-iterator-step-order.js` — the
    `[a = await 1, b = 2] = [undefined, 2]` case with an instrumented iterable
    proving the second step happens after resume, not before.
  - `async-function-array-destructuring-iterator-close-on-underconsumption.js`
    — normal-completion `IteratorClose` when the pattern doesn't drain the
    iterator (slice 6), matching the sibling pair
    `test262/test/language/statements/variable/dstr/ary-init-iter-close.js` /
    `ary-init-iter-no-close.js` for the sync case.
  - `async-function-array-destructuring-iterator-step-err-no-close.js` /
    `async-function-array-destructuring-iterator-val-err-no-close.js` — the
    two negative cases from slice 6 (`next()` throws / `.value` getter
    throws), matching `ary-ptrn-elem-id-iter-step-err.js` /
    `ary-ptrn-elem-id-iter-val-err.js`.
  - `async-function-array-destructuring-iterator-close-on-throw.js` — abrupt
    exit closes exactly once (slice 7).
  - `async-function-array-destructuring-rest-after-await.js` — slice 8.
  - `async-function-array-destructuring-nested-in-object.js` — slice 9,
    matching the issue body's explicit `{x: [a = await 1]}` example.
  - `async-generator-array-destructuring-default-await.js` — slice 10,
    matching the existing `async-generator-destructuring-default-await.js`
    naming from #709.
- **Everything else:** `cargo test --release` (unit tests in
  `generator_analysis.rs` / `generator_transform.rs` from slices 1-4, 9) and
  `uv run python scripts/run-custom-tests.py` for anything placed under
  `tests/` instead of `test262-extra/`.

## 6. Regression risk

- **Tree-walker hot paths unaffected for already-supported patterns.**
  `pattern_lowering_supported` only flips to `true` for array patterns that
  actually contain an `await`; a plain `var [a, b] = arr` still has
  `pattern_contains_suspension` return `false` at the top of
  `pattern_lowering_supported`, short-circuits to `true` (supported, trivially,
  same as today) without ever reaching the new `Pattern::Array` per-element
  arm, and `pattern_needs_lowering` (which additionally requires
  `pattern_contains_await`) stays `false` — so `contains_suspension`'s
  `Statement::Variable` arm doesn't route it into
  `transform_variable_declaration`'s lowering branch, and it's unaffected by
  either state-machine driver.
- **`for (var [a = await 1] = x;;)` stays on today's path, verified.**
  `transform_for_statement`'s `ForInit::Variable` branch only calls
  `transform_variable_declaration` when the *init expression itself*
  (`d.init`) contains a suspension — it does not consult
  `pattern_needs_lowering` at all. An init like `[]` has no suspension, so a
  `for`-head declarator with an awaiting array-pattern default is not newly
  routed into the lowering this plan adds; it remains exactly as broken (and
  exactly as out-of-scope, per the #709 ADR's existing gap list) as it is
  today. No behavior changes here — confirm this stays true with a regression
  test in slice 3 or 4 rather than assuming it.
- **`for-of`/`for-in` heads and `catch` params untouched.** Both bind through
  `ForInOfLeft`/`EnterCatch` directly in the drivers, never through
  `transform_variable_declaration`; this plan doesn't touch either.
- **`for_of_stack` reuse is the main coupling risk.** Pushing a `ForOfLoopState`
  for an array-pattern iterator record means it now participates in
  `for_of_depth` counting for any `break`/`continue`/`throw` that unwinds past
  it. Its `label_set` must be empty (nothing can `continue`/`break` to a
  destructuring pattern), which the existing `matches_continue_target` already
  handles correctly for an *unlabeled* continue only by chance of how it's
  invoked — verify the routing that computes `LoopControlTarget.for_of_depth`
  for surrounding real loops doesn't miscount when a pattern's frame is on the
  stack at the time a labeled loop's `continue`/`break` target is computed
  (i.e., that this frame is popped by the time any *real* loop control needs
  to route past it, or that being present doesn't change the depth arithmetic
  incorrectly). This is exactly the kind of thing slice 7's abrupt-exit test
  should catch if it's wrong, but call it out explicitly since it's easy to
  get subtly wrong without a test exercising a `break`/`continue` crossing an
  in-flight array-pattern default's `await`. This risk is also why §3 requires
  `Step`/`Drain` to pop this pattern's own entry the instant `[[Done]]`
  becomes true, rather than leaving the pop to `Finish` alone — an entry that
  outlives its pattern's own execution (e.g. a `...rest` drain that never
  reaches `Finish`) is exactly the kind of stale frame that would corrupt this
  depth counting for an unrelated, later `break`/`continue` in the same
  function.
- **GC rooting.** No `gc.rs` changes are planned (§3) — verify this holds once
  slice 2 is implemented; if the iterator temp needs anything
  `collect_for_of_stack_roots` doesn't already provide (it roots
  `outer_env`/`iteration_env`, not the iterator value itself — that's rooted
  separately via `gc_root_value`, called explicitly at `Init` time the same
  way `ForOfInit` already does), that's a sign the design in §3 needs
  revisiting, not a sign `gc.rs` needs a new case.
- **Bytecode fast path: not applicable.** `grep -rn "StateTerminator" src/interpreter/bytecode/` returns nothing — the bytecode compiler never sees generator/async-function state machines (confirmed empty), so this change has no bytecode blast radius.
- **Node-compat library harnesses.** Destructuring with `await` defaults inside
  array patterns is not a pattern the pinned library corpora
  (`decimal.js`, `acorn`, `zod`, etc.) are known to exercise; no targeted
  re-run planned beyond the standard `cargo test --release` gate, but if a
  library harness regresses in CI it's a signal this change's tree-walker
  fallback path (`emit_pattern_binding` for non-suspending elements) has a
  bug, not that the harness needs updating.

## 7. Out of scope

- **Object rest beside an awaiting sibling** (`{a = await 1, ...rest}`) —
  needs `CopyDataProperties` with a consumed-key exclusion list, where some
  excluded keys may themselves live in `$dstr_key` temps from a lowered
  computed key. Shares no machinery with the iterator-stepping work here
  (`pattern_lowering_supported`'s `ObjectPatternProperty::Rest => false` arm is
  untouched). Left as a follow-up; #725 nominally covers it too, so the
  implementation stage should say so explicitly in the PR description rather
  than silently dropping it, and a fresh issue should be filed if #725 is
  closed without it.
- **Catch parameters and for-in/of heads** (`catch ({a = await 5})`,
  `for (var [a = await 1] of …)`) — unchanged, per the #709 ADR's existing gap
  list; no state boundary exists at `EnterCatch`/`ForOfHead` today.
- **`for (var [a = await 1] = …;;)` initializers** — confirmed unaffected in
  §6, unchanged.
- **Destructuring *assignment* forms** (`[a = await 1] = []`) — the left side
  is an `Expression` already rewritten to `Yield` by the async-to-generator
  pass; `extract_lhs_suspensions` only handles `Member`, so this still hangs
  exactly as before. Untouched by this plan.
- **`yield` in a declaration pattern** (`var [a = yield 1] = []`, sync and
  async generators) — `pattern_needs_lowering` still requires `await`
  specifically; a yield-only array pattern keeps the existing replay path,
  unchanged.
- Refactoring the existing object-pattern lowering code in
  `generator_transform.rs` to share more structure with the new array-pattern
  code beyond what's needed for `lower_pattern_binding`'s dispatch — resist the
  urge to unify `lower_pattern_property`'s conditional-default shape into a
  fully generic helper in this PR; the two-line duplication between object and
  array default handling is small enough that forcing an abstraction now would
  cost more in review surface than it saves.
