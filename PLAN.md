# Plan: issue #721 — `for await` nested in try/if/block still blocks on `await_value`

## 1. Problem restated

`for await` performs `Await(nextResult)` on every iteration (spec: ForIn/OfBodyEvaluation),
which must suspend the enclosing async function/generator and resume only as a later
queued Job. JSSE implements this by lowering a `for await` loop into explicit
`ForOfInit`/`ForOfHead` states in the generator/async state machine — but only when the
lowering pass first *decides* the enclosing statement needs lowering at all. That
decision (`stmt_has_suspension` in `src/interpreter/generator_transform.rs`) special-cases
a `for await` only when it is the exact top-level statement handed to it. When a `for await`
is buried inside a `try`, `if`, bare block, or another loop's otherwise-non-suspending body,
the outer container is judged "no suspension" and is emitted verbatim, so the `for await`
inside it runs through the tree-walker's blocking `exec_for_of`/`await_value`, which only
drains already-queued microtasks instead of truly suspending. Observable effect: the loop
stops after its first `next()` once the iterator settles asynchronously (e.g. via a timer),
instead of resuming on each subsequent settle. This reproduces identically in `async function`
and `async function*`.

## 2. Spec basis

- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`**
  (ForIn/OfBodyEvaluation): step "If `iteratorKind` is `async`, set `nextResult` to
  `? Await(nextResult)`." This `Await` must happen, unconditionally, on every iteration of
  every `for await` loop — the abstract operation is invoked identically regardless of what
  statement lexically encloses the `ForOfStatement`.
- **`await`** (Await abstract operation): suspends the running execution context and returns
  control to the caller; the continuation resumes only as a queued Job, never inline. This is
  the behavior the tree-walker's `exec_for_of`/`await_value` path does not provide — it drains
  the microtask queue synchronously instead.
- **`sec-block-runtime-semantics-evaluation`** (Block), **`sec-if-statement-runtime-semantics-evaluation`**
  (IfStatement), **`sec-try-statement-runtime-semantics-evaluation`** (TryStatement): each just
  evaluates its nested `Statement` via ordinary recursive "Evaluation of `stmt`" — none of them
  special-case or suppress the suspension behavior of a nested `for await`. The container
  contributes nothing but its own completion-record plumbing, so JSSE's lowering must propagate
  "this subtree suspends" up through Block/If/Try/loop/Switch/Labeled/With the same way the spec's
  recursive Evaluation does, or the container's blocking-native fallback silently breaks the
  contract established by the first bullet.

This is a bug in JSSE's internal suspension-detection heuristic, not a case where the language
behavior itself is in question — the fix must make the engine's detection match what the spec
clauses above already require.

## 3. Files to touch

- `src/interpreter/generator_transform.rs` — the two functions responsible for suspension
  detection during generator/async lowering:
  - `stmt_contains_for_of_head` (~line 608–635): the `Statement::ForOf(f) => head(f)` arm does
    not recurse into `f.body`, so a `for await` nested inside an *outer loop's* body (sync
    `for`/`for-in`/`for-of`/`while`/`do-while`) is invisible to every caller of this helper,
    including the `create_simple_machine` early-out at ~line 528–541 and `stmt_contains_for_await`
    itself. Compare the sibling `stmt_contains_return` (~line 637–670), whose equivalent
    `Statement::ForOf(f) => stmt_contains_return(&f.body)` arm does recurse — this is the pattern
    to restore here.
  - `stmt_has_suspension` (~line 672–690): the `if let Statement::ForOf(f) = stmt && (...)`
    special case only inspects `stmt` itself, never a `for await` nested under it. Replace the
    direct `Statement::ForOf` match with a call to `stmt_contains_for_of_head(stmt, closure)`,
    where `closure` captures `is_async`/`detect_for_await` and reproduces the existing three
    disjuncts (`f.disposes_at_head()`, `detect_for_await && f.awaits_at_head()`,
    `f.is_await && !for_in_of_left_contains_suspension(&f.left)`) unchanged. Both captures are
    `bool`, so the closure is `Copy` and satisfies `stmt_contains_for_of_head`'s existing
    `impl Fn(&ForOfStatement) -> bool + Copy` bound with no signature change.
  - Inline `#[cfg(test)] mod tests` in this file (~line 3600+): re-run after the fix; some
    state-shape assertions (e.g. around `for_of_depth`/`try_depth` on `LoopControlTarget`,
    ~line 3625–3645) exercise sync generators with plain `yield` and should be unaffected, but
    must be checked since more containers now lower through the general state-machine path.
- `src/interpreter/generator_analysis.rs` — **no functional change planned.** `contains_suspension`
  (~line 1131) is deliberately left alone: it has no `is_async`/`detect_for_await` parameters
  and can't express the `detect_for_await`-gated disjunct, it's independently unit-tested
  (~line 1488–1671), and by the time `stmt_has_suspension`'s fallback reaches it, the new
  recursive pre-check above will already have returned `true` for any `for await`-bearing
  subtree. Do not move `stmt_has_suspension`'s logic here — the third disjunct's
  `!for_in_of_left_contains_suspension` guard is a transform-capability question ("can this
  head be lowered'), not a semantic "does this suspend" question, and belongs with the
  transform, where it already lives.
- `test262-extra/` — new regression tests (see §5).
- No `docs/adr/` entry: this is a bug fix in an existing, already-documented lowering
  mechanism, not a new architectural decision.

## 4. TDD slices

1. **Unit: `stmt_contains_for_of_head` recurses into a loop body.**
   In `generator_transform.rs`'s test module, construct (or parse via `parse_fn_body`, whichever
   the surrounding tests in this module already use) a statement shaped like
   `for (const x of xs) { for await (const y of ys) {} }` and assert
   `stmt_contains_for_await(&outer_stmt)` is `true`. Red against current code (arm doesn't
   recurse), green after restoring `head(f) || contains(&f.body)` on the `Statement::ForOf` arm.

2. **Unit: `stmt_has_suspension` sees a `for await` nested under `try`/`if`/block.**
   Same test module: build `try { for await (x of y) {} } finally {}`, `if (true) { for await
   (x of y) {} }`, and `{ for await (x of y) {} }` and assert `stmt_has_suspension(&stmt, true,
   true)` (async function shape) is `true` for each. Also assert it for `detect_for_await =
   false` when `f.is_await` is set (the async-generator shape), matching disjunct 3. Red before
   the `stmt_contains_for_of_head` rewrite in `stmt_has_suspension`, green after.

3. **Integration: the issue's exact repro, `async function` + `try`/`finally`.**
   New `test262-extra/async-function-for-await-nested-in-try-finally.js`, modeled on
   `test262-extra/async-generator-for-await-suspends-microtask-queue.js` (same `esid:
   sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`,
   `includes: [asyncHelpers.js]`, `flags: [async]`, `features: [async-iteration]`). Use
   `Promise.resolve().then()` tick markers around a multi-item async iterator (no `setTimeout` —
   host timers are for `tests/`, not `test262-extra/`; the promise-tick-order shape is what
   directly encodes "suspend, don't drain the queue inline"). Assert every iteration's log
   entry appears, interleaved with the marker ticks, and that `finally` runs last. Red before
   the fix (loop stops after one iteration), green after.

4. **Integration: `async function` + for-await nested in a sync loop's body.**
   New `test262-extra/async-function-for-await-nested-in-loop-body.js`: a `for (const x of [1,
   2]) { for await (const y of asyncIter) { ... } }` shape, same tick-marker assertion style.
   This is the slice that specifically exercises the `stmt_contains_for_of_head` recursion fix
   from slice 1 (a container the top-level `stmt_has_suspension` special case never covered even
   before this issue, since it isn't itself a `ForOf`/`If`/`Block`/`Try`).

5. **Integration: `async function*` + `try`/`finally` and `if`/block.**
   New `test262-extra/async-generator-for-await-nested-in-try-finally.js` and
   `test262-extra/async-generator-for-await-nested-in-if-and-block.js` (the latter covering
   both the `if (true) { }` and bare-block shapes in one file, two independently-logged
   sub-cases), driven by manual `.next()` calls as in the existing sibling
   `async-generator-for-await-*` tests rather than `asyncTest`, matching that file's harness
   convention. Confirms the fix holds for `detect_for_await = false` (disjunct 3's path), not
   just the `async function` (`detect_for_await = true`) path exercised by slices 3–4.

6. **Regression: loop-control unwinding through a lowered container.**
   Extend or sibling-file `test262-extra/async-generator-loop-control-closes-for-await-iterators.js`
   with a `try { for await (...) { break; } } finally { ... }` variant, confirming
   `LoopControlTarget`'s `try_depth`/`for_of_depth`/`scope_depth` book-keeping still closes the
   async iterator and runs the `finally` exactly once when the container the `for await` sits
   in is now lowered instead of native. This is the "verify the lowering of try/if/block/loops
   around a lowered for-await head" half of the issue's fix direction, not just the detection
   half.

## 5. Test surface

Targeted test262 re-runs (none of these currently encode this exact nested shape, per a search
of `test262/test/language/statements/for-await-of/` — its `nested` files are all about
destructuring nesting inside the loop's binding, not about the loop itself being nested in a
container — but they're the surface most likely to regress from touching shared lowering code):
- `test262/test/language/statements/for-await-of/`
- `test262/test/language/statements/async-function/`
- `test262/test/language/statements/async-generator/`
- `test262/test/language/expressions/async-function/`
- `test262/test/language/expressions/async-generator/`
- `test262/test/language/statements/class/async-gen-method*`, `class/async-method*`
- `test262/test/built-ins/AsyncGeneratorFunction/`
- `test262/test/annexB/language/statements/for-await-of/`

New `test262-extra/` files per §4, slices 3–6, following the existing
`async-generator-for-await-*.js` naming and header conventions (esid citing
`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`,
`features: [async-iteration]`).

Non-engine surfaces (`scripts/run-node-shim-selftest.sh`, `scripts/run-shim-fixtures.sh`,
library harnesses): not applicable — this change touches only generator/async lowering
internals, nothing those shims or libraries exercise directly.

After the targeted runs: full `uv run python scripts/run-test262.py` (per CLAUDE.md, required
after any implementation work) and `cargo test --release` for the unit/state-shape tests in
`generator_transform.rs`/`generator_analysis.rs`.

## 6. Regression risk

- **Blast radius is wide, not narrow.** The fix makes the lowering pass correctly recognize
  more subtrees as "must suspend," which moves whole `try`/`if`/block/loop bodies from native
  tree-walker execution to the state-machine path across *every* async function and async
  generator that contains a nested `for await` (or, per slice 1, a `for await` nested in an
  outer loop's body, previously invisible even to the early-out). This is not bounded to the
  handful of test262 directories above; a full suite run is the only way to bound it.
- **`LoopControlTarget` book-keeping** (`for_of_depth`, `try_depth`, `scope_depth`) must stay
  correct when a `break`/`continue` now unwinds out of a `for await` through a container that
  used to run natively. Slice 6 targets this directly.
- **`transform_try_statement`'s finalizer path** (`transform_clause_body`, ~line 2648–2660, and
  the `block_has_await_using` checks at ~2649/2714/2761) and the `Block` arm's
  `ScopeAction::OpenBlock`/`scope_depth` bump (~line 953–975) interact with a newly-lowered
  `for await`'s per-iteration lexical environment (`ForOfInit`/`ForOfHead` terminators) — these
  paths are already exercised by existing `try`/block lowering tests for `yield`, but not
  previously for a `for await` reached only via container recursion.
- **Existing inline state-shape unit tests** in `generator_transform.rs` may need their expected
  state graphs updated where they happen to contain a `for await` inside a container that now
  lowers where it previously didn't (unlikely, since none currently combine `for await` with an
  enclosing container by inspection, but must be checked after the change, not assumed clean).
- **Not a plausible mover:** the property MOP (`property.rs`), GC rooting/`gc_safepoint()`, the
  `ObjectKind` matches, the bytecode fast path (generator/async bodies never pass through
  `dispatch_body`, confirmed by grep — no `contains_suspension`/`stmt_has_suspension`/
  `generator_transform`/`generator_analysis` references anywhere under `src/bytecode/`), and the
  Node-compat library harnesses. None of these are touched or exercised by this change.

## 7. Out of scope

- **`for await` with a suspending binding target** (e.g. a destructuring default containing its
  own `await`/`yield`): disjunct 3's `!for_in_of_left_contains_suspension(&f.left)` guard
  continues to exclude this combination from the recursive fast path in async generators; it
  stays on the existing native/replay fallback (`generator_context`/`InlineYield` machinery,
  issue #625 territory) both before and after this fix. Not a regression — a deliberate
  non-change, called out here so it isn't mistaken for a miss.
- **The `generator_context`/`InlineYield` replay fallback in `generator_runtime.rs` generally** —
  untouched; this fix only changes what gets *detected* as needing lowering, not the lowering or
  fallback mechanisms themselves.
- **Moving `stmt_has_suspension` (or any of its disjuncts) into `generator_analysis.rs`** — the
  transform-capability guard belongs with the transform, not the analysis module; see §3.
- **`await using` semantics beyond what the recursion fix gives for free** — `disposes_at_head`
  detection now recurses into containers via the same `stmt_contains_for_of_head` fix, but no
  new `await using`-specific behavior is planned or should be bundled in.
- **Rolling `test262-pass.txt` forward** — not performed from this branch; that's a `main`-branch
  operation per CLAUDE.md.
- **Any refactor of `stmt_has_suspension`'s three disjuncts beyond the mechanical lift into a
  closure** (e.g. simplifying disjunct 3 now that disjunct 2 subsumes it for `detect_for_await =
  true` callers) — a legitimate future cleanup, but bundling it here would mix a refactor into a
  bug fix.
