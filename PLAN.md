# Plan: issue #828 — async-generator `yield*` error constructor identity swaps under GC stress

## 1. Problem restated

When an async generator evaluates `yield* obj` and `obj[%Symbol.asyncIterator%]`
exists but is not callable (or its `GetV`/call otherwise throws), the driver
in `generator_runtime.rs` that resolves the delegate's iterator catches the
`Err(e)` from `get_async_iterator` and, instead of propagating `e` directly,
retries by calling `self.get_iterator(&yield_val)` — a second, unrelated
fallible call that can run arbitrary user script (a `Symbol.iterator`
getter) and allocate. The original `TypeError` value `e` is held in a bare
Rust local across that nested call without being pushed onto
`gc_temp_roots`. A collection inside that nested call (reliably forced with
`$262.gc()` from within the getter, or incidentally under
`JSSE_GC_STRESS`) can find no root for `e`'s backing object, free it, and
recycle its arena slot for whatever the nested call allocates next. `e` then
reads back as that unrelated object (observed as the `Function` constructor
instead of `TypeError`), which is exactly the "wrong-typed-object" symptom
the issue describes, consistent with #824/#827. The fallback itself is also
wrong independent of GC: per `GetMethod`, a non-callable (or throwing)
accessor for `%Symbol.asyncIterator%` must throw directly and must never
fall back to `%Symbol.iterator%` — that fallback is only valid when
`GetMethod(obj, %Symbol.asyncIterator%)` *returns* `undefined`, a case
`get_async_iterator` already handles internally by returning `Ok(...)`
without ever reaching the buggy retry. Fixing the control-flow bug (never
retry on `Err`) removes the only GC-unsafe window in this path and
simultaneously restores spec-compliant behavior.

## 2. Spec basis

- `sec-generator-function-definitions-runtime-semantics-evaluation` —
  the `YieldExpression : yield * AssignmentExpression` algorithm: after
  `Let generatorKind be GetGeneratorKind()` and evaluating the operand, it
  does `Let iteratorRecord be ? GetIterator(value, generatorKind)`. The `?`
  means any abrupt completion from `GetIterator` is immediately the
  completion of the `YieldExpression`'s iterator-acquisition step — there is
  no alternate path, and in particular no second attempt at a different
  iterator source.
- `sec-getiterator` (`GetIterator ( obj: an ECMAScript language value, kind:
  ~sync~ or ~async~ )`), current text (`spec/spec.html:7018-7039`):
  1. If `kind` is `~async~`, then
     1. `Let method be ? GetMethod(obj, %Symbol.asyncIterator%)`.
     1. If `method` is `undefined`, then
        1. `Let syncMethod be ? GetMethod(obj, %Symbol.iterator%)`.
        1. If `syncMethod` is `undefined`, throw a `TypeError` exception.
        1. `Let syncIteratorRecord be ? GetIteratorFromMethod(obj,
           syncMethod)`.
        1. `Return CreateAsyncFromSyncIterator(syncIteratorRecord)`.
  The sync fallback is reached only when step 1.1's `GetMethod` call
  *returns* `undefined` (sub-step 1.2) — an abrupt completion out of that
  same `? GetMethod(...)` call (sub-step 1.1's `?`) propagates directly out
  of `GetIterator` and never reaches sub-step 1.2 at all.
- `sec-getmethod` (`GetMethod ( V, P )`), current text
  (`spec/spec.html:6278-6295`):
  1. `Let func be ? GetV(V, P)`. — an abrupt completion here (e.g. a
     throwing getter on the property itself) propagates immediately, before
     any callability check.
  1. `If func is either undefined or null, return undefined.`
  1. `If IsCallable(func) is false, throw a TypeError exception.` — this is
     the exact step the issue's repro exercises
     (`obj[Symbol.asyncIterator] = Symbol.asyncIterator`, a non-callable
     value); it throws without ever reaching `%Symbol.iterator%`.
  1. `Return func.`

## 3. Files to touch

- `src/interpreter/eval/generator_runtime.rs` — the async-generator `yield*`
  delegate-iterator resolution in the `StateTerminator::Yield { is_delegate:
  true, .. }` arm (currently lines ~4542–4554). Current code:

  ```rust
  let iterator = match self.get_async_iterator(&yield_val) {
      Ok(it) => it,
      Err(e) => match self.get_iterator(&yield_val) {
          Ok(it) => it,
          Err(_) => {
              self.retire_generator(o.id);
              let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
              return Completion::Normal(promise);
          }
      },
  };
  ```

  Fix: delete the inner `match self.get_iterator(&yield_val) { ... }` retry
  entirely and move its `Err` body (unchanged — still `retire_generator` +
  `call_function(&reject_fn, &JsValue::UNDEFINED, &[e])` +
  `return Completion::Normal(promise)`) directly into the outer `Err(e)`
  arm:

  ```rust
  let iterator = match self.get_async_iterator(&yield_val) {
      Ok(it) => it,
      Err(e) => {
          self.retire_generator(o.id);
          let _ = self.call_function(&reject_fn, &JsValue::UNDEFINED, &[e]);
          return Completion::Normal(promise);
      }
  };
  ```

  This is the only change. Do not swap in `route_exception!` /
  `reject_async_generator_request` here even though the sibling call sites
  listed below use those — see §9 for why that's a separate, larger change
  this issue does not take on. The sibling sites are cited only to show that
  "propagate `get_async_iterator`'s `Err` without retrying" is already the
  established pattern elsewhere: `src/interpreter/exec.rs:2352-2356`,
  `src/interpreter/eval.rs:9283-9290`, and
  `src/interpreter/eval/generator_runtime.rs:5127-5135`.
- `test262-extra/` — one new regression file (see §5).
- No `docs/adr/` entry: this is a localized control-flow bug fix in existing
  machinery, not an architectural decision.

## 4. TDD slices

1. **Red:** add the new `test262-extra/` file (§5), which forces the
   corruption deterministically (no `JSSE_GC_STRESS` sampling needed) by
   calling `$262.gc()` plus an allocation-churn loop from inside the
   `%Symbol.iterator%` getter that the current buggy retry wrongly invokes
   — the same technique `test262-extra/RegExp-split-splitter-construction-gc-rooting.js`
   uses for its own species-constructor rooting bug. Run it with
   `uv run python scripts/run-test262.py test262-extra/<new-file>.js`.
   Confirm it fails exactly as expected pre-fix: the non-callable-`@@asyncIterator`
   scenario shows the getter was invoked at all (counter `> 0`, violating
   the "must not fall back" assertion) and/or the rejection's `.constructor`
   corrupted; the undefined-`@@asyncIterator`-with-throwing-sync-getter
   scenario shows the getter invoked twice instead of once.
2. **Green:** apply the `generator_runtime.rs` fix described in §3. Re-run
   the same file — all assertions pass, including the `$262.gc()`-forced
   case, since the fix removes the call that could reach the getter at all
   in the non-callable scenario, and removes the double-invocation in the
   undefined-with-throwing-getter scenario.
3. **Issue repro cross-check:** build release, reproduce the issue's own
   repro (`yield-star-getiter-async-not-callable-symbol-throw.js` harness +
   body under `JSSE_GC_STRESS=2`) before and after the fix to confirm the
   constructor-identity corruption described in the issue is gone. This is a
   manual verification step for the implementation stage, not a separate
   committed test — the committed regression is the file from slice 1.
4. **Full-suite check (per project policy, not optional):**
   - `uv run python scripts/run-test262.py` — full suite, no stress.
   - `uv run python scripts/run-custom-tests.py`.
   - `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/ --timeout 300`
     and the same with `--bytecode`, mirroring `ci.yml`'s blocking stress
     job.
   - `./scripts/lint.sh`.
   - `cargo test --release`.

## 5. Test surface

- Targeted test262 directories to spot-check first (read-only; not
  modified) — the five `yield-star-getiter-async-not-callable-*.case`
  variants (`symbol`/`boolean`/`number`/`object`/`string`), which the
  project's generator expands to 60 files across these directories:
  - `test262/test/language/expressions/async-generator/`
  - `test262/test/language/statements/async-generator/`
  - `test262/test/language/expressions/class/async-gen-method/`
  - `test262/test/language/expressions/class/async-gen-method-static/`
  - `test262/test/language/statements/class/async-gen-method/`
  - `test262/test/language/statements/class/async-gen-method-static/`
  - `test262/test/language/expressions/class/elements/async-gen-private-method/`
  - `test262/test/language/expressions/class/elements/async-gen-private-method-static/`
  - `test262/test/language/statements/class/elements/async-gen-private-method/`
  - `test262/test/language/statements/class/elements/async-gen-private-method-static/`
  - `test262/test/language/expressions/object/method-definition/`
  Run each with `uv run python scripts/run-test262.py <dir>` (no stress) to
  confirm no baseline regression; this is a fast subset check ahead of the
  mandatory full-suite run in slice 4. Separately, re-run the `symbol` and
  `object` variants under `JSSE_GC_STRESS=2` to confirm the fix closes the
  issue's own reproduction.
- New `test262-extra/` file, suggested name
  `async-generator-yield-star-getiter-async-method-must-not-retry-sync-gc-rooting.js`,
  `esid: sec-getmethod`, `features: [host-gc-required]`. Two scenarios in
  one file (`asyncHelpers.js`, deterministic — no `JSSE_GC_STRESS` needed):
  - **Scenario A (non-callable async method):** `obj[%Symbol.asyncIterator%]`
    is a non-callable object (covers the general case plainly; the issue's
    own repro additionally used a Symbol, which test262 already covers).
    `obj`'s `%Symbol.iterator%` is a getter that increments a counter, calls
    `$262.gc()` plus an allocation-churn loop (forcing the retry's target
    slot to be reused if unrooted), and returns a harmless stub iterator
    (not a throw — a throw is what today's generated test262 case uses, and
    its result is silently discarded by the current bug, which is why
    test262 cannot catch this). After `iter.next()` rejects: assert the
    getter's counter is `0` (the forbidden fallback must never run) and
    assert the rejection reason's `.constructor` is `TypeError` (pins the
    issue's exact corruption symptom).
  - **Scenario B (undefined async method, throwing sync method):**
    `obj[%Symbol.asyncIterator%]` is `undefined`. `obj`'s `%Symbol.iterator%`
    is a getter that increments a counter, calls `$262.gc()` plus the same
    churn, and throws a distinguishable sentinel error object (tagged with
    an own property so identity corruption is directly observable, not just
    inferred from `.constructor`). `GetIterator`'s own internal sync
    fallback (inside `get_async_iterator`) must invoke this getter — but
    only once. After `iter.next()` rejects: assert the counter is `1` (not
    `2`, which is what today's buggy external retry produces) and assert
    the rejection reason is reference-identical to (or at least carries the
    same sentinel tag as) the object the getter threw, not a corrupted
    stand-in.
  This is the spec-correct behavior test262 cannot express today: its
  generated cases signal the forbidden access via a throw whose
  `Test262Error` is discarded by the current bug's own `Err(_) => { use the
  original e }` arm, so they pass today even though the forbidden getter
  *was* invoked. It belongs in `test262-extra/` per the project's policy for
  spec-correct behavior not covered by test262, and it still gets exercised
  by `ci.yml`'s existing blocking `JSSE_GC_STRESS=7` pass over
  `test262-extra/` (normal and `--bytecode`) on every future PR.
- No `tests/` addition: nothing here is an engine-internal heuristic,
  resource limit, or host-compatibility diagnostic — it is an observable
  ECMAScript throw-identity and side-effect question, which belongs in
  `test262-extra/`.

## 6. Regression risk

- **Baseline (`test262-pass.txt`):** very low risk of movement. Every
  `Err(e)` the removed retry could observe already represents a case
  `get_async_iterator` itself classifies as a genuine abrupt completion (not
  the "method is undefined" case, which `get_async_iterator` already
  resolves internally via `Ok(create_async_from_sync_iterator(...))` without
  ever returning `Err`). No spec-compliant test can depend on silently
  recovering from a non-callable/throwing `%Symbol.asyncIterator%` by
  falling back to `%Symbol.iterator%`, since the spec forbids exactly that.
  Verify with the directories in §5 run without stress, then the mandatory
  full-suite run in slice 4 — no `--update-baseline` planned or needed.
- **Why the fix is itself GC-safe:** after the fix, `e` is created by
  `get_async_iterator` and consumed by `self.call_function(&reject_fn, ...,
  &[e])` in the very next statement, with `self.retire_generator(o.id)` in
  between. `retire_generator` (lines 110+) only mutates the interpreter's
  own `HashMap`s and reconstructs an `IteratorState` enum value — it calls
  no `eval_expr`/`exec_statement`/`call_function` and therefore executes no
  JS statement. Per the GC Stress Mode discipline, collections fire only at
  statement-boundary/loop-back-edge safepoints, never at a bare Rust
  allocation; since no JS statement runs in that gap, no safepoint can fire
  in it, so `e` cannot be collected before `call_function` roots it as an
  argument. This matches the already-safe sibling sites in §3, which make
  the identical `Err(e) => ... call_function(&reject_fn, ..., &[e])` style
  of use with no intervening call.
- **Shared machinery leaned on:** `src/interpreter/eval/generator_runtime.rs`
  is the single place (confirmed by grep: no `get_async_iterator`/
  `AsyncGenerator`/`yield_star` references exist under
  `src/interpreter/bytecode/`) that drives async-generator `yield*` for both
  the lowered state-machine path and the inline-yield fallback described in
  the module's architecture notes — generators never compile through the
  bytecode VM, so there is no second implementation to mirror the fix into.
- **GC rooting / `gc_safepoint()`:** the fix is a net removal of a call,
  not an addition of new rooting bookkeeping, so it reduces the engine's
  exposure rather than adding a new root-stack discipline obligation.
- **`ObjectKind` exhaustive matches / property MOP (`property.rs`):**
  untouched — no new `ObjectKind` variant, no new MOP operation.
- **Node-compat library harnesses:** untouched — none of the pinned
  libraries' bundles exercise a non-callable `%Symbol.asyncIterator%` in a
  `yield*` position as part of their normal control flow; no re-run planned
  beyond the standard CI gate.

## 7. Prior-attempt workspace state (verified, not re-done)

This workspace already carries uncommitted progress toward this exact plan,
left by an earlier attempt. Verified by reading (no edits made in this
planning stage):

- `src/interpreter/eval/generator_runtime.rs` has an **uncommitted** working-tree
  diff that is precisely the §3 fix: the inner `match self.get_iterator(&yield_val)`
  retry is deleted and its `Err` body moved into the outer `Err(e)` arm. Nothing
  else in the file is touched.
- `test262-extra/async-generator-yield-star-getiter-async-method-must-not-retry-sync-gc-rooting.js`
  exists as an **untracked** file and matches §5 exactly: both scenarios (A:
  non-callable `%Symbol.asyncIterator%`, asserts `getterCallsA === 0` and
  `resultA.value.constructor === TypeError`; B: `%Symbol.asyncIterator%`
  `undefined` with a throwing `%Symbol.iterator%` getter, asserts
  `getterCallsB === 1` and `resultB.value === sentinel`), the `$262.gc()` +
  64-iteration allocation-churn technique, `esid: sec-getmethod`, and
  `features: [host-gc-required]`.
- Both are untouched by this planning stage per the exit contract. The
  implementation stage inherits them as-is.
- **Slice-1 ("red") caveat:** because the fix is already applied in the
  working tree, running the new test now would pass immediately and never
  demonstrate failure against the unfixed code. Before trusting the test as a
  real regression guard, the implementation stage must first prove it fails
  pre-fix: `git stash push -u -m 828-prefix-check -- src/interpreter/eval/generator_runtime.rs`
  (or save/restore via `git diff ... > $TMPDIR/828.patch` + `git apply -R`),
  rebuild, run the new `test262-extra/` file, confirm scenario A's
  `getterCallsA` assertion (or scenario B's double-invocation) fails, then
  restore the fix and re-run to confirm green.

## 8. Fix-safety verification (read-only checks performed during planning)

Three claims the §6 regression-risk argument depends on were checked directly
against the current source (all read-only; no files edited):

- **`get_async_iterator` already resolves the "method is undefined" case
  internally and never routes it through the retry being deleted** —
  confirmed at `src/interpreter/builtins/iterators.rs:4567-4605`. When the
  `%Symbol.asyncIterator%` property read is nullish, `iter_fn` is `None` and
  the function falls through to its own `self.get_iterator(obj)?` fallback
  (line 4603), returning `Ok(create_async_from_sync_iterator(...))`. This path
  never returns `Err`, so it can never reach the generator_runtime.rs retry
  being deleted.
- **A non-callable `%Symbol.asyncIterator%` value (scenario A) already
  produces `Err` from inside `get_async_iterator` itself, without any
  internal sync fallback** — `get_async_iterator` calls
  `self.call_function(&iter_fn, obj, &[])` directly (no `IsCallable` guard of
  its own); `call_function_inner_impl`'s terminal fallthrough for a
  non-callable value (`src/interpreter/eval.rs:5814-5816`) constructs a
  `TypeError` ("... is not a function") and returns `Completion::Throw(err)`,
  which `get_async_iterator` maps to `Err(e)` at line 4597. So the deleted
  retry's `Err(e)` arm in scenario A was already unreachable-via-fallback at
  the `get_async_iterator` layer — the bug was purely the outer retry
  needlessly firing anyway.
- **The retry pattern being deleted is the only one of its kind** — every
  call site of `get_async_iterator` was enumerated
  (`grep -rn "get_async_iterator(" src/`): `src/interpreter/eval.rs:9284`
  (tree-walker `for await`), `src/interpreter/exec.rs:2353` (tree-walker
  `for await`), `src/interpreter/eval/generator_runtime.rs:5124` (async
  generator's own `for await`), and the one being fixed at
  `generator_runtime.rs:4543` (`yield*`). The other three already match the
  "propagate `Err` directly, no `get_iterator` retry" pattern the fix adopts
  (`exec.rs:2353-2356` returns `Completion::Throw(e)` directly; `eval.rs:9284-9290`
  and `generator_runtime.rs:5124-5135` both assign `pending_exception`/call
  `reject_async_generator_request` directly). No other occurrence of this bug
  exists in the engine.

## 9. Out of scope

- Refactoring the ad hoc `GetMethod`/`GetV` inlining duplicated across
  `get_iterator`, `get_async_iterator`, and other call sites in
  `src/interpreter/builtins/iterators.rs` into one shared helper. Tempting
  given this bug lives in that duplication, but it is a horizontal refactor
  touching many call sites for a fix that only needs one.
- Routing this block's three `Err` arms (`GetIterator`'s own failure, the
  `next` property-get failure, and the `next` call failure) through
  `route_exception!`/the generator's own try/catch/finally instead of
  rejecting the result promise directly. The module's own comment at
  `generator_runtime.rs:3026-3034` suggests these failures should propagate
  as the ordinary abrupt completion of the `YieldExpression` (through
  enclosing `try`/`finally`), which the direct-reject pattern used uniformly
  across all three arms arguably does not do — but that is pre-existing,
  uniform across the whole block, and a materially larger change than
  deleting one erroneous retry. Worth a follow-up issue, not bundled here.
- Auditing `get_iterator`'s and `get_async_iterator`'s own internal
  `GetMethod`/`GetV` sequences for unrelated unrooted-intermediate bugs
  beyond the one call site this issue identifies. If the stress runner finds
  another one, it becomes its own issue (as #824/#827/#828 already are).
- Rewording or "fixing" the generated test262 case itself — it is
  spec-correct; the bug is in the engine's control flow, not the test.
