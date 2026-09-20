# Plan: issue #670 — for-in with a suspension point in a state-machine body yields nothing

## 1. Problem restated

`transform_for_in_statement` in `src/interpreter/generator_transform.rs` is a
stub: it emits `Statement::Empty` ("For-in with yields is complex - for now emit
as-is"). Any `for-in` whose body (or RHS) contains a suspension point is routed
there by `transform_yielding_statement`, so the whole loop is silently dropped.
A for-in with **no** suspension in it is emitted verbatim and runs under the
tree-walker's `exec_for_in`, which is why only the yielding shape is broken.

Reproduced on this branch (release build at `5e8ee75b`) — the stub is shared by
every state-machine driver, so the bug is wider than the issue title:

| shape | jsse | expected |
|---|---|---|
| `function* g(o){ for (var k in o) yield k }` | `[]` | `["a","b"]` |
| `async function f(o){ for (var k in o) { await 0; r.push(k) } }` | `[]` | `["a","b"]` |
| `async function* g(o){ for (var k in o) yield k }` | `[]` | `["a","b"]` |
| `let` head, nested for-in, labelled `continue`, `break`, `for (k in (yield 1))` | all wrong (loop skipped) | |

The same transform also serves top-level-await module bodies
(`stmt_has_tla` in `src/interpreter/mod.rs` already lists `ForIn`), so a module
`for (k in o) { await … }` is affected as well.

## 2. Spec basis

- §14.7.5 For-In, For-Of, and For-Await-Of Statements
  - `sec-runtime-semantics-forinofloopevaluation` — ForInOfLoopEvaluation
    (`for (… in …)` = ForIn/OfHeadEvaluation with `~enumerate~`, then
    ForIn/OfBodyEvaluation with `~enumerate~`). (Old id
    `sec-for-in-and-for-of-statements-runtime-semantics-labelledevaluation`
    named in the issue.)
  - `sec-runtime-semantics-forinofheadevaluation` — TDZ env for lexical head
    names while the RHS is evaluated; `undefined`/`null` RHS → `~break~`
    completion (loop body never runs); otherwise `ToObject` then
    `EnumerateObjectProperties`, then `GetV(iterator, "next")`.
  - `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`
    — per-iteration lexical environment for `let`/`const` heads; for
    `~enumerate~` an abrupt body completion returns **without** IteratorClose.
  - `sec-enumerate-object-properties` — the enumerator is "never directly
    accessible to ECMAScript code"; its `throw`/`return` are *null* and never
    invoked; a property deleted before it is processed is skipped; keys are
    Strings only, at most once, prototype-chain shadowing applies.
  - `sec-createforiniterator`, `sec-%foriniteratorprototype%.next` — reference
    behaviour the enumerator must not deviate from.
- §27.5.3.3 `sec-generatorresume` / `sec-generatoryield` — a `yield` inside the
  loop body suspends the generator and resumption continues the enumeration at
  the same position; the enumerator state and the active iteration environment
  must survive the suspension. (Async analogues: Await §6.2.3.1 /
  AsyncGeneratorYield for the other two drivers.)
- Annex B.3.5 (`sec-initializers-in-forin-statement-heads`): sloppy
  `for (var i = init in o)` evaluates the initializer once, before the RHS.

## 3. Design (decision + alternatives)

**Chosen: lower for-in onto the existing for-of state-machine machinery, with a
spec-shaped internal "For-In Iterator" as the iterator.**

The spec itself models for-in as an iteration over an internal iterator object
(`EnumerateObjectProperties`) whose `return` is null. The for-of lowering
already provides everything the loop needs across suspension and abrupt exits,
in all three drivers: `ForOfInit`/`ForOfHead` terminators, the runtime
`for_of_stack` (`ForOfLoopState`), per-iteration `iteration_env`, TDZ head env
(`for_of_head_tdz_env`), `try_depth`, `LoopControl` targets/`for_of_depth`,
unwinding on `break`/`continue`/`return`/throw/`generator.return()`. Re-deriving
those for a second loop kind means re-doing six driver sites of hard-won code.

Concretely:

1. **`IteratorState::ForInEnumerator { obj_id, keys, index }`** (new variant in
   `src/interpreter/types.rs`) plus a hidden `%ForInIteratorPrototype%` realm
   field with **`[[Prototype]]` = null** and a single builtin `next`. `next`
   steps `keys[index]`, skips keys for which `proxy_has_property(obj_id, key)`
   is false (same as `exec_for_in` today), returns
   `create_iter_result_object(key, false)`, and `{undefined, true}` at the end.
   Null prototype is load-bearing: the drivers' generic `iterator_close` does
   `GetMethod(iterator, "return")`; with a null-proto enumerator that is always
   `undefined`, so user-installed `Object.prototype.return` /
   `Iterator.prototype.return` can never fire (spec: enumerate never closes).
2. **`Interpreter::for_in_head_iterator(&mut self, &JsValue) -> Result<JsValue, JsValue>`**
   — the spec's ForIn/OfHeadEvaluation `~enumerate~` branch: nullish → an
   already-exhausted enumerator (observably identical to the `~break~`
   completion: no user code runs, loop yields nothing, generator continues at
   `after_state`); otherwise `to_object`, collect keys with the *same* code
   `exec_for_in` uses, allocate the enumerator. Extract that key-collection
   block out of `exec_for_in` (the `needs_proxy_path` /
   `proxy_enumerable_keys_with_proto` / `enumerable_keys_with_proto_on_id`
   branch) into one helper used by both the tree-walker and the enumerator so
   the two can never diverge. This is the only edit to `exec_for_in`; behaviour
   unchanged.
3. **`StateTerminator::ForOfInit` gains an `is_for_in: bool`** (only `Init`; the
   `ForOfHead` terminator is untouched — it just calls `iterator_next` on
   whatever object is in `iter_var`). In each of the three drivers the single
   `get_iterator(&iterable_val)` call becomes
   `if is_for_in { self.for_in_head_iterator(&v) } else { self.get_iterator(&v) }`
   (sync generator `eval/generator_runtime.rs` ~L1482, async generator
   `eval/generator_runtime.rs` ~L5373, async function/module driver
   `eval.rs` ~L9039). Also update the `clear_terminator_ic_sites` /
   destructuring sites in `generator_transform.rs` that mention `ForOfInit`.
4. **GC**: `gc.rs::trace_object_fields` is an exhaustive match — the new
   `IteratorState` variant must push `obj_id` to the worklist (mirror
   `ArrayIterator { array_id, .. }` at `gc.rs` ~L1007). The new realm prototype
   field must be added to the realm root list in `types.rs` (~L659, next to
   `array_iterator_prototype`) and initialised in `setup_iterator_prototypes`.
   The enumerator itself is rooted by the existing `gc_root_value(&iterator)` +
   `iter_var` binding in the drivers. Keys are Strings only (no Symbols), so no
   other traced references.
5. **Transform**: implement `transform_for_in_statement` by extracting the body
   of `transform_for_of_statement` into a shared helper
   `transform_for_in_of_loop(left, right, body, is_await, is_for_in, ctx,
   after_state)` and calling it from both. The for-in variant must:
   - hoist a suspending RHS into a temp exactly like `forof_iterable`;
   - keep `ctx.for_of_depth += 1` around the body — at runtime the loop *is* on
     `for_of_stack`, so the transform-time depth must match (this is what keeps
     `LoopControl`/`try_depth` correct, including async functions using
     `detect_for_await`);
   - pass `is_await: false, is_for_in: true`;
   - handle the Annex B initializer: if `left` is `Variable(var)` with
     `declarations[0].init`, emit that as a normal `var` declaration statement
     (via `transform_variable_declaration` so a suspending initializer works)
     before `ForOfInit`, and pass a copy of the declaration with `init: None`
     as `left`.
6. No change to `generator_analysis.rs`, `hoisting.rs`, the parser, or the
   bytecode path (the state-machine terminators are not compiled by
   `bytecode/`; confirm by grep at implementation time). Verified while
   planning:
   - `contains_yield` / `contains_suspension` `ForIn` arms already check
     `f.right` **and** `f.body` (`generator_analysis.rs` L698, L896), so a
     for-in whose *only* suspension is in the RHS reaches the transform; slice 2
     carries a test for exactly that shape.
   - `iterator_close` / `iterator_close_result` (`builtins/iterators.rs`
     ~L4961/L4992) already treat an `undefined`/`null` `return` as a no-op, so
     the null-prototype enumerator makes every close path inert with no driver
     edits. Slice 5 pins this.
   - `ForOfHead` head binding for `Variable` (bind_pattern, `Var`/`Let`/`Const`
     kinds, per-iteration `iteration_env`) and `Pattern`
     (`assign_to_for_pattern`, covering identifiers, member/index targets and
     destructuring) already behaves correctly for for-of — checked
     empirically (`for (t.p of …)`, `for (a[0] of …)`, `for ([k] of …)` and
     strict-mode undeclared identifier → ReferenceError all work in a
     generator on this branch) and the parser produces the same
     `ForInOfLeft` shapes for for-in. `ForInOfLeft::Expression(_)` is a no-op
     arm in the drivers ("handled via assignment"); the implementer must
     confirm the parser never yields `Expression` for a valid for-in head
     (parser `statements.rs` ~L954/L971), otherwise add the
     `assign_to_expr` handling that `exec_for_in` has. Differences from
     `exec_for_in` that are *not* worth replicating: its "evaluate then throw
     ReferenceError for an invalid LHS" arm is unreachable because invalid
     targets are early errors (covered by test262 syntax tests).

**Alternatives rejected**
- *New `ForInInit`/`ForInHead` terminators with keys+index in temp bindings*:
  duplicates iteration-env, `try_depth`, unwind and per-driver plumbing for a
  second loop kind; larger and more bug-prone than reusing the for-of stack.
- *Pure-AST lowering to `Goto`/`ConditionalGoto`*: needs an expression that
  enumerates keys — no way to spell `EnumerateObjectProperties` without a new
  `Expression` variant (invasive: parser/analysis/IC-clear/bytecode matches) or
  an observable global helper (violates "never accessible to ECMAScript code").
- *Add `is_for_in` handling to `ForOfHead`/all close paths*: more edits, and
  every `iterator_close` site would need auditing; the null-proto enumerator
  gets the spec's "no IteratorClose" behaviour for free.
- *Own `next` function per enumerator instead of a shared prototype*: allocates
  a function object per loop execution; the realm-level hidden prototype is the
  spec shape (`%ForInIteratorPrototype%`) and avoids that.

Judgement call to note in the PR: the spec's `%ForInIteratorPrototype%` has
`[[Prototype]]` = `%Iterator.prototype%`; we deliberately use null because it is
unobservable (never reachable from script) and it makes `IteratorClose` inert
without touching the drivers. Behaviour the spec *does* specify is preserved.

## 4. Files to touch

- `src/interpreter/generator_transform.rs` — replace the stub; extract the
  shared for-in/for-of lowering; add `is_for_in` to `StateTerminator::ForOfInit`
  and update its other mentions (`clear_terminator_ic_sites`, any `Debug`/clone
  destructures).
- `src/interpreter/types.rs` — `IteratorState::ForInEnumerator`, realm field
  `for_in_iterator_prototype` (+ root-list entry).
- `src/interpreter/gc.rs` — trace the new `IteratorState` variant.
- `src/interpreter/builtins/iterators.rs` — hidden prototype + `next` builtin in
  `setup_iterator_prototypes`; `for_in_head_iterator`.
- `src/interpreter/exec.rs` — extract the key-collection helper from
  `exec_for_in` (no behaviour change).
- `src/interpreter/eval/generator_runtime.rs` (two `ForOfInit` arms) and
  `src/interpreter/eval.rs` (one `ForOfInit` arm) — `is_for_in` branch.
- Any other exhaustive `IteratorState` match the compiler flags (grep
  `IteratorState::` — e.g. debug/inspection helpers).
- New tests under `test262-extra/` (see §5).
- `CLAUDE.md`/`AGENTS.md`: no change needed (the "InlineYield fallback" note is
  unaffected). No ADR: no architectural decision beyond reusing the for-of
  machinery; no new domain vocabulary beyond "For-In Iterator" which is spec
  terminology.

## 5. TDD slices

Each slice: write the test first, confirm it fails on the current binary, make
it pass, run `cargo build --release` → `uv run python scripts/run-test262.py
test262-extra/`. Keep every new test under `test262-extra/` in test262 style
(frontmatter with `description`, `esid`, `info`, `features`, `includes:
[compareArray.js]`, `flags: [async]` + `$DONE` for async cases), modelled on
`generator-for-of-per-iteration-environments.js`.

1. **Sync generator, `var` head — the reported bug.**
   `test262-extra/generator-for-in-yield-in-body.js`
   (esid `sec-runtime-semantics-forinofloopevaluation`): `for (var k in o) yield
   k` → `["a","b"]`; `for (k in o)` with an outer, previously declared `k`;
   non-`Identifier` targets (`for (obj.p in o)`, `for (arr[0] in o)`);
   own-then-prototype key order with shadowing; symbols excluded; string
   primitive RHS `for (i in "ab")`; statements after the loop still run;
   strict-mode `for (undeclared in o)` → ReferenceError from `.next()`.
   Production: the enumerator, `for_in_head_iterator`, `is_for_in` on
   `ForOfInit`, the shared transform helper, and the **sync-generator driver**
   arm only. (This is the smallest end-to-end vertical slice; it forces the
   GC/type plumbing, so run `cargo build` and the clippy hook after it.)
2. **Nullish and TDZ head.** Same file or
   `generator-for-in-head-evaluation.js`
   (esid `sec-runtime-semantics-forinofheadevaluation`):
   `for (k in null|undefined) { yield 1 }` yields nothing and the generator
   continues; `for (let k in (yield 1))` — RHS suspension, resumed value is what
   gets enumerated; **and a case whose only suspension is the RHS** with a
   yield-free body (`for (var k in (yield o)) { log.push(k) }`), which must
   still take the state-machine path; `for (let k in k)`-style TDZ ReferenceError is thrown from
   the generator (and routed to an enclosing `try`/`catch` inside it). Production:
   RHS-hoist path of the shared helper (already sketched in §3.5).
3. **Lexical heads and per-iteration environments.**
   `test262-extra/generator-for-in-per-iteration-environments.js`
   (esid `sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`):
   `for (let k in o) yield () => k` and `const`/destructured heads produce
   closures with distinct bindings; nested for-in inside for-in and inside
   for-of keeps every active iteration env across `yield`. Production: none
   expected (falls out of reusing `ForOfLoopState.iteration_env`); fix
   `for_of_head_lexical` only if a gap shows.
4. **Abrupt exits.**
   `test262-extra/generator-for-in-abrupt-exits.js`:
   `break`, `continue`, labelled `break`/`continue` to an outer loop, `return`
   inside the body, `throw` inside the body caught by an outer `try`, and
   `try/finally` inside the loop body around a `yield`; plus
   `gen.return(v)` and `gen.throw(e)` while suspended inside the loop
   (finalizers run, generator completes). Production: none expected; if the
   `for_of_depth`/`try_depth` accounting is wrong this is where it shows.
5. **No IteratorClose on for-in** (spec: enumerate never closes).
   `test262-extra/generator-for-in-does-not-close-enumerator.js`
   (esid `sec-enumerate-object-properties`): install throwing/recording
   `return` on `Object.prototype` and `Iterator.prototype` (restore in
   `finally`), then `break`, `return`, throw, and `gen.return()` out of a
   yielding for-in — the recorder must stay empty. Production: none expected
   (null-proto enumerator); if red, the prototype chain is wrong.
6. **Enumeration semantics under suspension.**
   `test262-extra/generator-for-in-enumeration-across-yield.js`
   (esid `sec-enumerate-object-properties`): a property deleted between two
   `next()` calls (while suspended) is skipped; a property added is not
   required (assert only the spec-mandated part: no duplicates, deleted ones
   absent); a `Proxy` target routes `ownKeys`/`getOwnPropertyDescriptor`/`has`
   through traps once per key, matching the tree-walker; a throwing proxy trap
   surfaces from the generator's `.next()` and is catchable by a `try` around
   the loop.
7. **Async generator driver.**
   `test262-extra/async-generator-for-in-yield-in-body.js` (`flags: [async]`,
   `features: [async-iteration]`): the issue shape with `for await` consumer,
   `let` head closures, `await` in the body, break/labelled continue, and
   `return()` while suspended. Production: the `is_for_in` branch in the
   async-generator `ForOfInit` arm.
8. **Async function (and module TLA) driver.**
   `test262-extra/async-function-for-in-await-in-body.js` (`flags: [async]`,
   `features: [async-functions]`): same matrix with `await`, including
   per-iteration closures and an `await` in the RHS. Add
   `test262-extra/module-tla-for-in-await-in-body.js` (`flags: [module,
   async]`) only if an existing `module-tla-*` test in that directory shows the
   pattern works for the harness. Production: the `eval.rs` `ForOfInit` arm.
9. **Annex B initializer.**
   `test262-extra/generator-for-in-annexb-var-initializer.js`
   (esid `sec-initializers-in-forin-statement-heads`, `flags: [noStrict]`):
   `for (var i = f() in o) { yield i }` evaluates `f()` once, before the RHS,
   even when the RHS is nullish; initializer containing `yield` suspends before
   the loop. Production: the initializer branch in the shared helper.
10. **GC rooting.** `test262-extra/generator-for-in-gc-rooting.js` (naming
    follows `Array-length-set-gc-rooting.js`): a generator enumerating a
    freshly created wrapper (`"abc"` primitive → String wrapper, and an object
    with no other reference) with allocation-heavy work between `yield`s so a
    collection happens while suspended; assert all keys still come out.
    Production: only if red — indicates the `gc.rs` trace or realm root is
    missing.
11. **Refactor step (green → clean).** Confirm `exec_for_in` and
    `for_in_head_iterator` share the extracted key-collection helper, run
    `./scripts/lint.sh`, `cargo clippy` gate hook, `cargo test --release`.

## 6. Test surface

Targeted test262 runs (all must stay green; none exercise the new path at
runtime because test262 has no *runtime* for-in-with-yield-in-generator tests —
the only for-in+yield/await files are early-error/syntax tests — hence the
`test262-extra/` slices above):

- `test262/test/language/statements/for-in/` (incl. `dstr/`) — guards the
  `exec_for_in` key-collection extraction.
- `test262/test/language/statements/for-of/`, `for-await-of/` — guard the
  extracted shared transform helper and the `ForOfInit` field addition.
- `test262/test/language/statements/{generators,async-function,async-generator}/`,
  `language/expressions/{yield,await,generators,async-generator,async-function}/`
- `test262/test/language/module-code/top-level-await/` — TLA uses the same transform.
- `test262/test/annexB/language/statements/for-in/` — Annex B initializer.
- `test262/test/built-ins/{GeneratorPrototype,AsyncGeneratorPrototype}/`,
  `built-ins/Iterator/` (guards the new hidden prototype not leaking into
  iterator-related enumeration).
- `test262-extra/` whole directory (expected 100% green).
- `cargo test --release` (unit/integration, incl. `tests/`), `./scripts/lint.sh`.
- Full `uv run python scripts/run-test262.py` at the end (baseline read from
  `origin/main`; **do not** pass `--update-baseline`).

Not covered by test262, therefore new `test262-extra/` files: everything in
slices 1–10 (all runtime for-in-with-suspension behaviour). No `tests/` file
needed: every asserted behaviour is an observable ECMAScript value/throw.

## 7. Regression risk

- **`test262-pass.txt` baseline**: expect no regressions, possible *gains* only
  in `test262-extra` (not in the baseline) — a for-in inside a generator that
  previously vanished may now execute code some test262 test unknowingly relied
  on being skipped; run the full suite and diff against the baseline.
- **Shared machinery leaned on**: `for_of_stack`/`ForOfLoopState`
  (per-iteration env, `try_depth`, `LoopControl`, `for_of_depth` symmetry
  between transform time and runtime — an off-by-one here mis-targets
  `break`/`continue` across `finally`); the three near-duplicate driver
  `ForOfInit` arms (change all three identically); `iterator_next` /
  `iterator_close` generic paths (rely on `next` present and `return` absent);
  `iterator_next_cache` keyed by iterator id (make sure a stale entry cannot
  survive id reuse — check while implementing).
- **Exhaustive matches**: new `IteratorState` variant (compile-time enforced in
  `gc.rs`, and any other match without `_`), new realm field (root list — GC
  bug if forgotten; slice 10 guards it), new `ForOfInit` field (every
  destructure site).
- **`exec_for_in` extraction**: tree-walker hot path for every non-generator
  for-in; keep it a pure move, gate with the for-in test262 dirs.
- **Bytecode fast path** (`bytecode/`, off by default): untouched; state-machine
  terminators are interpreter-only.
- **Node-compat library harnesses** (`scripts/run-library-tests.sh`): libraries
  using for-in inside generators/async functions (e.g. acorn, uglify-js,
  luxon, zod) were silently mis-executing before; run at least
  `./scripts/run-library-tests.sh acorn` and `decimal.js` as a smoke check if
  time allows — a change in their counts is expected to be upward only.
- **Performance**: allocation of one enumerator per suspended for-in loop and
  one iter-result object per step; only the previously-broken path pays it.

## 8. Out of scope

- Any change to `for-of`/`for-await` behaviour beyond the mechanical extraction
  of the shared transform helper (no renames, no cleanup of the
  `#[allow(dead_code)]` fields on `ForOfInit`).
- A `yield`/`await` inside the *LHS* of a for-in/for-of head
  (`for ((yield).x in o)`): the for-of lowering does not decompose it either;
  file a follow-up issue rather than widening this fix.
- Making the tree-walker's `exec_for_in` lazy (spec's `CreateForInIterator`
  visited-set walk) instead of eager key collection — existing behaviour kept.
- Replacing the InlineYield fallback (#625), the bytecode VM, or the
  `$name_N` temp-variable naming scheme.
- Updating `test262-pass.txt`, formatting sweeps, dependency changes.

## 9. Follow-ups (to file after this lands)

- LHS-with-suspension in for-in/for-of heads.
- Lazy/spec-exact `CreateForInIterator` behaviour (mutation of the prototype
  chain during enumeration) shared by tree-walker and enumerator.
- Consider giving `%ForInIteratorPrototype%` the spec-accurate
  `%Iterator.prototype%` parent once enumerate loops skip IteratorClose
  structurally rather than via a null prototype.
