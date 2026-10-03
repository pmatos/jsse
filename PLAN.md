# Plan: issue #794 — in-flight Throw/Return completion payload is unrooted while finally/IteratorClose runs user code

## 1. Problem restated

jsse's GC only treats a `JsValue` as live if it is reachable from an explicit
root: an environment, the object graph, or `Interpreter::gc_temp_roots` (the
LIFO temp-root stack pushed/popped via `gc_root_value`/`gc_unroot_value`/
`with_gc_root_scope`). Several places in the tree-walker and the generator
state machine hold the payload of an in-flight abrupt completion (the value
being thrown, or returned, or an error being propagated through
`IteratorClose`) in a plain Rust local while they call into code that can run
arbitrary user JS — a `finally` block, or an iterator's `return()` method.
Nothing roots that local, so if a collection happens during that nested call
(confirmed empirically below, no `JSSE_GC_STRESS` required) the payload's
backing object can be freed, and when its arena slot is later reused by an
unrelated allocation the resumed completion silently observes the wrong
object. Three repros against the current branch, all on `target/release/jsse`
built from HEAD, no stress flag:

```js
// (a) issue body's literal repro — exec_try, Throw payload
try { try { throw new Error('x') } finally { $262.gc() } } catch (e) { print(e.message) }
// prints "undefined" (should print "x")

// (b) exec_try, Return payload
function f() { try { return {m:'r'} } finally { $262.gc() } }
print(JSON.stringify(f()));
// throws "TypeError: Converting circular structure to JSON" (should print {"m":"r"})

// (c) IteratorClose during for-of Return, exec_for_of_loop
function g() {
  var it = { [Symbol.iterator]() {
    var i = 0;
    return { next() { return {done: i++ > 2, value: i}; },
             return(v) { $262.gc(); return {done:true}; } };
  } };
  for (const x of it) { return {m:'from-for-of'}; }
}
print(JSON.stringify(g()));
// prints {"done":true} — aliases return()'s own freshly-allocated iterator
// result object, which was given the freed slot (should print {"m":"from-for-of"})
```

Under `JSSE_GC_STRESS=1` the same class of bug reproduces inside the
generator/async-generator state machine (`src/interpreter/eval/generator_runtime.rs`),
where `.throw()`/`.return()` injection and inline-tracked for-of iterator
cleanup hold their own transient locals unrooted across the same kind of call.
Verified against HEAD with `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py <file>`:
every file named in the issue fails except `generator-for-of-iterator-gc-rooting.js`
and `generator-for-of-per-iteration-environments.js`, which already pass (they
test iterator-liveness, a different invariant, already covered by the existing
`with_iter_close_scope`/`gc_root_value(&iterator)` discipline). Sample failure
(`generator-conditional-goto-abrupt-completions-through-try.js`, under stress):
`Actual [caught:undefined, after] and expected [caught:boom, after]` — the
thrown string payload is lost in exactly the same way as repro (a), but via
the state machine's own for-of/IteratorClose cleanup rather than `exec_try`.

The fix is mechanical and local at each site: root the completion's payload
(and, where relevant, iterators displaced from a GC-rooted collection by a
`.remove()`) for the dynamic extent of the call that can run user code, then
unroot. `src/interpreter/dispose.rs`'s `DisposeCursor` already does this
correctly (`step()` wraps everything in `with_gc_root_scope` plus
`for_each_value`); this plan applies the same idiom to the sites that don't.

## 2. Spec basis

- **`sec-try-statement-runtime-semantics-evaluation`** (`TryStatement : try Block Finally` / `try Block Catch Finally`): `B`/`C` (the try or catch completion) must still exist, unmodified, when `UpdateEmpty(F, undefined)` runs after `Finally` evaluates — i.e. the engine's representation of that completion must survive the finally block's evaluation. A GC that silently invalidates `B`/`C`'s payload mid-finally violates this literally (the resulting value is not the one the algorithm names).
- **`sec-iteratorclose`** (IteratorClose) and **`sec-asynciteratorclose`** (AsyncIteratorClose): both take a `completion` argument that must be returned unchanged (`? completion` / step 5's "return ? completion") after `Call(innerResult, return, iteratorRecord.[[Iterator]], « »)` — a call that runs arbitrary user code. The payload must outlive that call intact.
- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`** (ForIn/OfBodyEvaluation): steps that process `break`/`return`/`continue`/a binding-assignment `Throw` out of the loop body call `IteratorClose`/`AsyncIteratorClose` with that completion before returning it — same requirement, applied once per abrupt exit kind.
- **`sec-generatorresumeabrupt`** / **`sec-asyncgeneratorresume`**: a `.return(value)`/`.throw(exception)` injected into a suspended generator becomes that generator's resumption completion, which then propagates through any enclosing `finally`/for-of exactly as the bodies above require; the *value itself* must survive from the moment it's taken as the method argument to the moment it's either delivered to a handler or returned to the caller.

No JS syntax or semantics change: this is a GC-liveness bug in the host implementation of already-correct algorithms, not a reinterpretation of any clause.

## 3. Files to touch

Engine only, all under `src/interpreter/`:

- `src/interpreter/types.rs` — add a small `Completion` helper for GC rooting (used by #2 and #4 below).
- `src/interpreter/exec.rs`:
  - `exec_try` (~line 2518): root the pending completion's payload across the `finally` block's execution.
  - `bind_pattern`'s `Pattern::Array` arm (~line 1499–1636): root `error` across `iterator_close_result` (~line 1625).
  - `exec_for_of_loop` (~line 2359–2516): root the LHS-binding-step error across each `iterator_close` call (~lines 2417, 2425, 2435, 2443, 2451, 2458, 2491), and root the loop-carried value across each `iterator_close_result` call in the `Break`/`Return`/`Continue` arms (~lines 2479, 2485, 2495, 2506).
- `src/interpreter/eval.rs`:
  - `destructure_array_assignment` (~line 4176): root `error` across `iterator_close_result` (~lines 4360, 4364) — the assignment-target sibling of the `bind_pattern` fix above.
  - `close_for_of_iterator` (~line 9855): root the `completion` parameter's payload across `iterator_close_result` (~line 9880). This single fix also covers `unwind_generator_for_of_loops`'s (generator_runtime.rs:6846) for-of-closing step, since it calls through `close_for_of_iterator_parking` → `close_for_of_iterator` for the non-parking (synchronous) case.
- `src/interpreter/builtins/iterators.rs`:
  - `Interpreter::iterator_close` (~line 4963): root its `completion` parameter across `get_object_property("return")` and `call_function`. This is the central fix: every other caller of `iterator_close` in the codebase (`array.rs`, `builtins/mod.rs`, `builtins/promise.rs`, `builtins/collections.rs`, `builtins/intl/listformat.rs`, `eval/generator_runtime.rs` — roughly 30 call sites) passes either `JsValue::UNDEFINED` or `e.clone()` and then uses its own separately-held copy of `e`; `JsValue` is `Copy`/`Clone` over a NaN-boxed id (`src/types.rs:17`), so a cloned object value carries the *same* arena id as the original, and rooting the id inside `iterator_close` keeps the caller's own copy alive too. No other caller needs to change **except** the two documented below, which don't pass a clone of the value they go on to use.
- `src/interpreter/builtins/mod.rs`: `Math.sumPrecise`'s iterator loop (~line 2577) passes `JsValue::UNDEFINED` to `iterator_close` while its own `e` is a real (and more spec-accurate) completion value — pass `e.clone()` instead, which is simultaneously more correct per `IteratorClose(iteratorRecord, completion)` (the completion should carry the actual throw) and, via the central fix above, closes this hazard.
- `src/interpreter/builtins/array.rs`: `Array.from`'s iterable path (~lines 3024, 3035) has the same `JsValue::UNDEFINED`-while-holding-a-real-value pattern (`other: Completion` from the mapper call; `e` from `create_data_property_or_throw`) — pass the real payload instead of `UNDEFINED`.
- `src/interpreter/eval/generator_runtime.rs`:
  - `generator_return_state_machine` (~lines 2425–2509): the "close any iterators left open by inline yield" loop pulls `iters: Vec<JsValue>` out of `self.generator_inline_iters` via `.remove(&o.id)`, which is exactly the map `gc.rs` scans as a root (`gc.rs:488`); once removed, iterators not yet closed in the loop are unrooted Rust locals. `value` (the `.return(value)` argument, read again at line 2509 after the loop) is also never rooted. Root the whole batch (iterators + `value`) for the loop's duration.
  - `async_generator_next_state_machine_impl`'s injected-`.return()` resume path (~lines 4179–4227): the identical `generator_inline_iters.remove(...).into_iter().find_map(iterator_close_result)` pattern, plus `ret_val` (taken via `pending_return.take()` at line 4179, reused at line 4227) held unrooted across that same closing loop. Same fix shape as above.
- `test262-extra/` — new regression files (see §5).

No `docs/adr/` entry: this is a bugfix within the already-documented GC-rooting discipline (`docs/adr/2026-09-10-2014-gc-root-scope-guard.md` and the `GC Root-Stack Discipline` section of `CLAUDE.md`), not a new architectural decision. No `CONTEXT.md` change: no new vocabulary.

## 4. TDD slices

Each slice: write the test (red on current HEAD, verified above or to be verified the same way), then the minimal production fix (green), run `./scripts/lint.sh` and the targeted test before moving on.

1. **`exec_try` — Throw and Return payload across `finally`.**
   Test: `test262-extra/try-finally-abrupt-completion-gc-rooting.js`, two scenarios in one file (the `(a)` and `(b)` repros above, using the `collect()`-churn pattern from `generator-for-of-iterator-gc-rooting.js` rather than a bare `$262.gc()`, so a freed slot is actually reused rather than merely swept). `esid: sec-try-statement-runtime-semantics-evaluation`, `features: [host-gc-required]`.
   Fix: add `pub(crate) fn root_payload(&self, f: impl FnMut(&JsValue))` (name tbd at implementation time) to `Completion` in `types.rs`, matching `DisposeCursor::for_each_value`'s existing match (`Normal`/`Return`/`Throw`/`Yield` → `f(v)`; `Break(_, Some(v))`/`Continue(_, Some(v))` → `f(v)`); in `exec_try`, wrap the finalizer's execution in `self.with_gc_root_scope(|interp| { result.root_payload(|v| interp.gc_root_value(v)); interp.exec_statements(finalizer, &fin_env) })`.

2. **`bind_pattern` array-destructuring IteratorClose — binding-side.**
   Test: extend `test262-extra/generator-for-of-iterator-gc-rooting.js`'s existing `makeIterable` helper pattern (or a new sibling file, e.g. `array-pattern-binding-abrupt-error-gc-rooting.js`) with a case where stepping/binding an array-pattern element throws a heap-allocated error while the pattern's iterator's `return()` churns the heap. `esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset` is the closest ForIn/OfBodyEvaluation analog cited by the existing destructuring tests; follow whatever `esid` the destructuring spec section itself uses if more precise (check `sec-destructuring-binding-patterns-runtime-semantics-bindingpattern` family during implementation).
   Fix: in `exec.rs`'s `Pattern::Array` arm, root `error` (`with_gc_root_scope`/`gc_root_value`+`gc_unroot_value`) around the `iterator_close_result` call at line ~1625.

3. **`exec_for_of_loop` — every IteratorClose call site.**
   Test: `test262-extra/for-of-abrupt-completion-payload-gc-rooting.js`, covering: (i) throw during LHS binding/assignment with a churning `return()`, reusing repro-style iterables; (ii) `return` from inside the loop body with a heap-allocated return value, matching repro `(c)` above; (iii) labeled `break`/unlabeled `break` with a heap-allocated completion value; (iv) `continue` to an outer label that closes the loop. `esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`, `features: [host-gc-required]`.
   Fix: wrap each of the hazard call sites named in §3 in `with_gc_root_scope`, rooting the value that must survive the `iterator_close`/`iterator_close_result` call.

4. **`destructure_array_assignment` — assignment-pattern sibling of slice 2.**
   Test: sibling case in the same new file from slice 2, using `[a, b] = iterable` (assignment target) instead of `var [a, b] = iterable` (binding).
   Fix: same shape as slice 2, applied to `eval.rs`'s `destructure_array_assignment`.

5. **`close_for_of_iterator` — resumed/generator-driven for-of close.**
   Test: a generator whose `for (const x of iterable) { yield x; }` loop is driven to a `return`/`break` completion from outside (`gen.return(value)` after at least one `yield`), with the iterable's `return()` churning the heap. This exercises `close_for_of_iterator` via `close_for_of_loop`/`close_for_of_iterator_parking`/`unwind_generator_for_of_loops`. Extend `generator-for-of-iterator-gc-rooting.js`-style coverage or add `generator-for-of-return-completion-gc-rooting.js`.
   Fix: root `completion`'s payload across `iterator_close_result` in `close_for_of_iterator`.

6. **Generator/async-generator state-machine `.throw()`/`.return()` injection.**
   Test: this is the slice the five currently-stress-failing `test262-extra` files already cover —
   `generator-conditional-goto-abrupt-completions-through-try.js`,
   `generator-switch-abrupt-completions-through-try.js`,
   `async-generator-conditional-goto-abrupt-completions-through-try.js`,
   `async-generator-switch-abrupt-completions-through-try.js`,
   `async-generator-yield-star-abrupt-exit-closes-outer-for-of.js`,
   plus `generator-for-of-abrupt-exit-closes-iterators.js` and `generator-for-of-explicit-return-cleanup.js` and `generator-for-of-head-abrupt-completions.js` (all seven confirmed failing under `JSSE_GC_STRESS=1` against current HEAD; re-run after slices 1–5 to see how many are already fixed by the shared `close_for_of_iterator`/`iterator_close` fixes before touching `generator_runtime.rs` at all — several of these go through exactly those functions). For whatever remains red, also add at least one **deterministic** (non-stress) `test262-extra` regression exercising `generator_return_state_machine`'s/`async_generator_next_state_machine_impl`'s own `generator_inline_iters` + pending-value hazard directly (a generator with an *inline* — not state-machine-lowered — open for-of at the point `.return(value)` is injected, `value` heap-allocated, the open iterable's `return()` churning the heap), e.g. `generator-return-injected-value-gc-rooting.js`.
   Fix: in `generator_return_state_machine` and the async equivalent, root the batch of not-yet-closed `iters`/filtered iterators plus the carried `value`/`ret_val` for the duration of the closing loop (`with_gc_root_scope` wrapping the loop, rooting every iterator still in the collected list up front since none of them are reachable once `.remove()`'d from `generator_inline_iters`).

Run `uv run python scripts/run-custom-tests.py` is not relevant here (no `tests/` additions planned — see §5); `cargo test` is unaffected unless a unit test for `Completion::root_payload` is worth adding alongside the `types.rs` change (optional, small, same slice as #1).

## 5. Test surface

- Targeted test262 run: none required — this bug is not reachable from any `test262/` test without `$262.gc()` or `JSSE_GC_STRESS`, and test262 itself contains no GC-timing assertions. No `test262/test/...` directory is specifically implicated; a full default run (`uv run python scripts/run-test262.py`) still gates for regressions in the touched control-flow paths (try/catch/finally, for-of, destructuring, generators) since those are heavily exercised by `language/statements/try`, `language/statements/for-of`, `language/statements/for-of/...destructuring...`, and `built-ins/GeneratorPrototype`/`AsyncGeneratorPrototype` — run these directories targeted in addition to the full suite:
  - `test262/test/language/statements/try/`
  - `test262/test/language/statements/for-of/`
  - `test262/test/built-ins/GeneratorPrototype/`
  - `test262/test/built-ins/AsyncGeneratorPrototype/`
  - `test262/test/language/expressions/destructuring-assignment/` (array patterns)
- New `test262-extra/` files (per §4, slices 1–6): deterministic, no stress flag required, following the established `collect()`-churn + `$262.gc()` + `features: [host-gc-required]` convention seen in `generator-for-of-iterator-gc-rooting.js`. These are the permanent regression net (stress mode is not part of default CI per `CLAUDE.md`'s GC Stress Mode section).
- Verification-only (not a new permanent gate, but required before closing out slice 6): re-run the seven already-existing stress-sensitive files under `JSSE_GC_STRESS=1 uv run python scripts/run-test262.py test262-extra/<file>.js` and confirm all seven now pass, plus a broader stress sample (`JSSE_GC_STRESS=16 uv run python scripts/run-test262.py test262-extra/ --timeout 300`) to catch anything the targeted files miss.
- `cargo build --profile release-checked` + `uv run python scripts/run-test262.py --binary target/release-checked/jsse test262-extra/` — the root-stack balance `debug_assert!`s (`gc_assert_root_depth`, LIFO `gc_unroot_id`) are the mechanism most likely to catch a mismatched root/unroot pair introduced by this fix; run both the plain and `--bytecode` flag, though analysis below shows `--bytecode` should be a no-op for every touched function.
- `cargo test` (debug) for the `Completion` helper if a unit test is added in slice 1.
- Not applicable: `scripts/run-node-shim-selftest.sh`, `scripts/run-shim-fixtures.sh`, `scripts/run-library-tests.sh <lib>` — this change touches no Node-compat shim or library-harness surface.

## 6. Regression risk

- **Tree-walker hot paths**: `exec_try` and `exec_for_of_loop` are on the hot path for every `try`/`for-of` statement executed. The fix adds a `gc_root_frame`/`gc_unroot_frame` pair (or a `with_gc_root_scope` closure) only on the already-slow-path arms (abrupt completions, `finally` present) — the common case (`Completion::Normal` with no `finally`, or a loop that runs to exhaustion) is untouched, so no hot-path overhead.
- **Bytecode fast path**: not a risk. The bytecode compiler (`src/interpreter/bytecode/compiler.rs`) has no compiling arm for `Statement::Try` or `Statement::ForOf` (only a bail-reason label); any function containing either unconditionally bails to the tree-walker. The `--bytecode` CI run therefore exercises the identical tree-walker code path this plan changes, with no separate bytecode-side behavior to verify.
- **GC rooting / `gc_safepoint()`**: the main risk is a new root/unroot imbalance (an early `return` that roots but skips the matching unroot). `with_gc_root_scope` is deliberately preferred over hand-paired `gc_root_value`/`gc_unroot_value` wherever a call site has multiple exit paths (all the `exec_for_of_loop` match arms), since its `Drop`-free bulk-truncate-on-every-exit design can't leak a root the way a manually-threaded unroot can. The `release-checked` debug-assertions (`gc_assert_root_depth`, LIFO-order `gc_unroot_id`) are exactly the safety net for this class of mistake — run before considering any slice done.
- **`generator_inline_iters` / exhaustive `ObjectKind` match**: no new `ObjectKind` variant and no change to `trace_object_fields`'s match arms — the generator_runtime.rs fix only changes *when* iterators are rooted (via `gc_temp_roots` instead of transiently via the map), not what the GC walker traces, so the exhaustive-match compile guard is undisturbed.
- **Node-compat library harnesses**: no interaction — none of the touched functions are reachable from the Node-compat shims, and no library test exercises `$262`/`JSSE_GC_STRESS`.
- **`test262-pass.txt` baseline**: this fix should only ever turn prior silent-corruption failures into passes (or leave already-passing tests unchanged); it has no code path that could make a previously-passing test262 scenario observably different, since in the non-GC-triggering case every touched function's control flow is identical to today (the only change is additional root-stack bookkeeping around calls that were already being made). Flag any surprise diff against `origin/main:test262-pass.txt` in the PR description rather than rewriting the baseline (not ours to roll forward from this branch).

## 7. Out of scope

- **The `async_from_sync_continuation` closure family** (`builtins/iterators.rs` ~4715–4795, `%AsyncFromSyncIteratorPrototype%` plumbing: `sync_for_next`/`sync_for_return`/`sync_for_throw`/`outer_clone1`/`outer_clone2`). Investigation during planning found these native closures are never registered via `pin_native_root`/`gc_native_roots`, unlike structurally similar closures elsewhere in the same file (`create_iterator_helper_object`, `zip_fn`) — a captured-value rooting gap, but a different bug class (missing closure-capture rooting, not an in-flight-completion payload) and a different fix shape (pin the closure's captures, not root a local across a call). Worth its own issue.
- **`generator_throw_state_machine`'s `yield*`-delegation path** (`generator_runtime.rs` ~2512 onward, e.g. the `exception` parameter passed to `self.iterator_throw(&iterator, &exception)` at line ~2563): investigation found the delegate `iterator` itself stays reachable because the live object's `kind` isn't mutated until after the hazardous call in the branches checked, but a full audit of every branch in this and the async equivalent (`async_generator_next_state_machine_impl`, ~2200 lines) was not completed — slice 6's empirical stress-test re-run after slices 1–5 is the gate for whether more of this file needs the same treatment. If the seven stress-sensitive files are not all green after slice 6, file a follow-up issue scoped to whatever remains, rather than open-endedly auditing the rest of `generator_runtime.rs` in this PR.
- **The `.throw()`/`.return()` resume paths' own argument liveness before reaching `pending_exception`/`pending_return`/`try_stack[..].pending_completion`** (which *is* correctly GC-traced per `gc.rs:520-524,1179-1183` once stored there): only the two confirmed gaps in §3/§4 slice 6 are in scope; do not do a line-by-line rewrite of `generator_runtime.rs`'s rooting discipline as part of this bugfix.
- **Mechanical `iterator_close(&iterator, JsValue::UNDEFINED)` → real-payload sweep beyond the two confirmed hazards** (`builtins/mod.rs` `Math.sumPrecise`, `builtins/array.rs` `Array.from`): every other `iterator_close` call site passes `e.clone()`/moves `e` in and is already safe once `iterator_close` itself roots its parameter (JsValue is `Copy` over an arena id, so the clone and the original share GC identity) — do not touch `promise.rs`, `collections.rs`, `intl/listformat.rs`, or the `generator_runtime.rs` `e.clone()` call sites; they need no change.
- **Refactoring `DisposeCursor`, `with_iter_close_scope`, or any other already-correct rooting idiom** — this plan reuses them as-is, it doesn't touch them.
- **Rolling `test262-pass.txt` forward** (`--update-baseline` is a `main`-branch operation, not part of this PR).
