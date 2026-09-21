# Plan: issue #679 — JetStream async-fs: File's DataView field reads back undefined mid-run

## 1. Problem restated

In JetStream's `generators/async-file-system.js`, `setupDirectory()` (a plain
`async function`) runs `for await (const fileContents of randomFileContents())`
where `randomFileContents` is an `async function*` yielding a fresh `DataView`
each turn, and the loop body itself contains a second, unrelated `await`
(`await dir.addFile(...)`) before looping back to fetch the next value. Under
sufficient allocation volume, `fileContents` intermittently binds to
`undefined` instead of the yielded `DataView`, even though nothing in the
JetStream source ever assigns `undefined` to it. The `File` built from that
turn stores `this._data = undefined` (a perfectly ordinary property write),
and the failure only becomes visible later, when `byteLength`/`swapByteOrder`
dereferences `this._data` and throws `TypeError: Cannot read properties of
undefined (reading 'byteLength')`. Because nothing awaits the outer IIFE's
promise in the original JetStream driver, that `TypeError` becomes an
unhandled rejection that jsse drops silently, exiting 0 with no output — which
is how `run-jetstream.py` reported this as "no JSON output" (#655) before it
was split out as this bug.

### Diagnosis performed in this planning stage (evidence, not yet a fix)

Built `target/release/jsse` at HEAD and confirmed:

- The issue's exact repro (`/tmp/afs_catch.js`, tree-walker, 1 call to
  `runIteration`) **no longer fails at HEAD** — 5/5 clean runs (`C`, `D`).
  Numerous generator/GC-rooting fixes have landed since jsse 0.8.2
  (`#690`–`#692`, `#688`, `#672`, `#666`, `#658`, `#604`, `#499`, `#473`,
  among others). This bug is **not fixed**, but the *specific* repro command
  in the issue body needs updating for whoever verifies the fix.
- `target/release/jsse --bytecode /tmp/afs_catch.js` (1 iteration) reproduces
  the exact reported error **3/3**, deterministically. `--bytecode` does not
  compile `await`/`yield`-bearing bodies (only `compiler.rs` even mentions
  `await`; `vm.rs` has no suspend/resume path), so this is not a bytecode-VM
  defect — bytecode just raises allocation pressure per wall-clock iteration
  enough to expose a shared-machinery bug faster.
- The default (tree-walker) engine reproduces the **same bug family**
  reliably (3/3) once given more allocation volume: driving
  `b.runIteration(i)` for `i` in `0..6` fails at `i == 2` every time, with a
  related but different downstream symptom (`Cannot convert undefined or
  null to object`, from `Directory` code that assumes a defined value).
  This confirms the defect is allocation/GC-timing dependent and lives in
  code shared by both dispatch paths, not in `bytecode/`.
- Patched a scratch copy of `async-file-system.js`'s `File.prototype.data`
  getter to probe a File whose `.data` reads back `undefined`:
  `Object.keys(this)` includes `"_data"`, `Object.prototype.hasOwnProperty
  .call(this, "_data")` is `true`, and `Reflect.get(this, "_data")` — which
  bypasses any inline cache — **also** returns `undefined`. The property is
  present with a stored value of `undefined`; this is not a case of a
  GC-recycled object id resolving `_data` to an unrelated live object (that
  would read back as some *other* object, not `undefined`), and not a stale
  inline-cache slot pointing at the wrong index (`Reflect.get` doesn't
  consult the IC and agrees).
- Patched the `for await` loop body directly to print `typeof fileContents`
  and `File._data` immediately after `new File(fileContents)`. Confirmed
  **`fileContents` itself is already `undefined`** at construction time, at
  `fileCounter == 96` and `fileCounter == 578` in one 800-file run. The fault
  is upstream of `File`/`DataView` entirely: it is in the `for await`
  loop's iteration-result delivery.
- `iterator_next` (`src/interpreter/builtins/iterators.rs:4919`) only checks
  `v.is_object()`; `iterator_complete`/`iterator_value`
  (`iterators.rs:4954`/`4966`) read `.done`/`.value` off whatever object they
  are given and silently default to `false`/`undefined` if those properties
  are absent — they do not validate that the object is actually the
  `IteratorResult` produced by this turn's `next()` call. So if the value
  the state-machine driver treats as "the settled await result" for the
  `for-await-of` head is ever *not* that `IteratorResult` (stale, wrong, or a
  still-pending promise), the loop silently manufactures `{done: false,
  value: undefined}` with no thrown exception — exactly the observed
  behavior.
- The most concrete suspect is the `for await` head's own await/resume
  protocol: `StateTerminator::ForOfHead`'s handling in
  `src/interpreter/eval.rs` (~lines 9166–9224) distinguishes "first entry"
  from "resumed after await" by checking whether a synthetic binding named
  `format!("{iter_var}__await")` in `func_env` is non-`undefined`, then
  resets it to `JsValue::UNDEFINED` once consumed. That binding is populated
  by the generic async-function resume path
  (`async_fn_suspend_at_await`, `eval.rs:9466`, writing through
  `SentValueBinding`/`AsyncFunctionState.pending_binding`,
  `src/interpreter/generator_transform.rs:248`,
  `src/interpreter/types.rs:376`, `src/interpreter/scheduler.rs:269-283`).
  This loop's body has its *own*, separate `await` in between visits to the
  head, so on every iteration this async function suspends/resumes twice
  through the shared machinery. This is the exact shape (`for await` +
  another `await` in the loop body) that #691/#692/#690/#688/#672 have been
  incrementally hardening for `yield`; the equivalent hardening for this
  `await`-in-body-of-a-for-await-of combination has not been shown correct.

This is as far as a planning stage should go without writing code. The two
concrete branches implementation must resolve, in order, are in TDD slice 1
below.

## 2. Spec basis

This is a JS-behavior-affecting engine-internal bug (an observably wrong
property/iteration-variable value), not a change to JavaScript syntax or
semantics — jsse must continue to behave exactly as required by the following
clauses; the fix makes it do so:

- **OrdinaryGet ( O, P, Receiver )**, ECMA-262 §10.1.8.1
  (`spec/spec.html#sec-ordinaryget`) and **OrdinarySet** / **OrdinarySetWith
  OwnDescriptor**, §10.1.9.1/§10.1.9.2
  (`spec/spec.html#sec-ordinaryset`, `#sec-ordinarysetwithowndescriptor`): a
  stored own data property must read back the value most recently written to
  it. No clause permits an engine implementation detail (GC timing,
  allocation volume, inline caching) to change what `[[Get]]` returns for an
  unmodified own property. `file._data` reading back `undefined` after the
  constructor wrote a `DataView` to it, without any intervening
  `_data = undefined`, violates this regardless of which layer is at fault.
- **ForIn/OfBodyEvaluation** (`spec/spec.html
  #sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`,
  ECMA-262 §14.7.5.7): for `iteratorKind` = ~async~, each iteration must bind
  the loop variable to `IteratorValue(?Await(IteratorNext(iteratorRecord)))`
  — the `[[value]]` of *that* turn's own iterator-result, never a stale or
  unrelated value. jsse's state-machine lowering of `for await` must preserve
  this per-iteration correspondence even when the loop body contains its own
  suspension points.
- **Await ( value )**, ECMA-262 §27.7.5.3
  (`spec/spec.html#await`, under Async Function Abstract Operations): the
  execution context (including all local bindings) must be suspended and
  later resumed with exactly the value the awaited promise settled with.
  Two sequential `Await`s within one async function body (the implicit await
  inside `for await`'s `IteratorNext`, then the explicit `await
  dir.addFile(...)`) must not cross-deliver each other's settled values.

## 3. Files to touch

Engine (all under `src/interpreter/`):

- `eval.rs` — `StateTerminator::ForOfHead` await handling (~9122-9224) and
  `async_fn_suspend_at_await` (~9466). Primary fix site if slice 1's probe
  shows the resume-value delivery itself is wrong.
- `generator_transform.rs` — `SentValueBinding`/`SentValueBindingKind`
  (~248-260) and the `ForOfInit`/`ForOfHead` terminator lowering for
  `for await` combined with a suspending loop body (~2236-2310 and the
  `detect_for_await`/`stmt_contains_for_await` helpers). Fix site if the
  defect is in how the compiler decomposes this specific loop shape rather
  than in the runtime delivery.
- `scheduler.rs` — `AsyncFunctionState`/`insert_async_function_state`/
  `iter_async_function_states` (~260-290) only if slice 1 shows the pending
  resume value or `for_of_stack` entry is lost between suspend and resume
  (a rooting/bookkeeping gap), not a pure logic error in eval.rs.
- `gc.rs` — `collect_gc_roots` / write-barrier helpers only if slice 1's
  bisection lands on a genuine missing-root window (see Regression risk);
  do not touch speculatively.
- `builtins/iterators.rs` — `iterator_complete`/`iterator_value`
  (~4954-4973) are candidates for tightening (rejecting a non-`IteratorResult`
  rather than silently defaulting) once the upstream defect is understood,
  but tightening these alone would only turn the symptom into a `TypeError`
  at the right place — it is not by itself the fix for #679 unless the root
  cause turns out to be unfixable at the delivery layer (unlikely; treat as
  a secondary hardening, not slice 1's target).

Non-engine:

- None required. This is not a `scripts/`/CI/benchmark-harness gap; #681
  already closed the runner-side "no JSON output" misclassification for this
  family. No `docs/adr/` entry unless implementation lands on a real
  architectural change to the suspend/resume protocol (see Out of scope).

## 4. TDD slices

1. **Decide which branch the defect is on, with a named failing test.**
   Add `tests/for_await_of_nested_await_value_delivery.rs` (a `#[test]`
   driving `jsse` via the existing in-process interpreter harness used by
   other `tests/*.rs` integration tests — see `src/interpreter/tests.rs` for
   the harness pattern) that runs:
   ```js
   async function* source() {
     let i = 0;
     while (true) { yield { n: i++ }; }
   }
   async function drive(iterations) {
     let last;
     for await (const item of source()) {
       await Promise.resolve(); // second await inside the loop body
       if (item === undefined || item.n === undefined) {
         throw new Error(`corrupted at iteration ${last}`);
       }
       last = item.n;
       if (last >= iterations) break;
     }
     return last;
   }
   ```
   run for enough iterations to force at least one minor and one major
   collection (reuse the nursery/major thresholds from `gc.rs` to size the
   loop, e.g. a few thousand iterations plus incidental allocation), and
   assert it completes without throwing and returns the expected final
   count. This is expected to be **red** today under `--bytecode`-equivalent
   allocation pressure (or, if the harness runs the tree-walker only, at a
   high enough iteration count) — confirm redness before touching production
   code. If it does *not* go red at any reasonable iteration count, the
   defect is specific to something else in `async-file-system.js` (e.g. the
   `Directory`/`Map` bookkeeping, or `swapByteOrder`'s own allocation
   pattern) — stop, re-diagnose with the same probe technique used in this
   plan (patch a scratch copy of the *actual* benchmark file, not the
   distilled test), and update this slice before proceeding.
2. **If slice 1 reproduces: force it deterministically and cheaply.**
   Replace the free-running `source()` above with a custom async iterable
   whose `next()` returns a thenable that calls `$262.gc()` synchronously
   inside its own `then` callback before resolving
   (`{ next() { return { [Symbol.for('async-iterator-result-thenable')]: 1,
   then(resolve) { $262.gc(); resolve({ value: { n: i++ }, done: false }); } } } }`
   — `$262` is always defined by `setup_globals()`
   (`src/interpreter/builtins/mod.rs:4171`), no test262 harness include
   needed). This targets the exact window between `IteratorNext`'s promise
   settling and the state machine consuming it. If this reproduces in a
   handful of iterations, slice 3 gets a **deterministic** test; if it does
   not reproduce even with forced GC at that exact point, the defect is not
   a GC-rooting gap in that window, and the fix is a pure control-flow/value
   -delivery bug in `eval.rs`'s `ForOfHead`/`async_fn_suspend_at_await`
   pairing — proceed straight to slice 4 with a large-iteration-count
   regression instead.
3. **(Only if slice 2 confirms a GC-timing dependency.)** Add the
   deterministic reproduction from slice 2 as
   `test262-extra/language/statements/for-await-of/nested-await-value-not-collected-during-suspend.js`,
   following existing test262 file header conventions (`esid`, `description`,
   `info` citing ForIn/OfBodyEvaluation and Await, `flags: [async]`,
   `includes: [doneprintHandle.js]` or the plain `$262`/`print` pattern
   already used elsewhere in this repo's `test262-extra/`), and fix the
   rooting/write-barrier gap in `eval.rs`/`scheduler.rs`/`gc.rs` (whichever
   slice 1/2 bisection pointed at) so the deterministic test goes green
   without weakening `iterator_complete`/`iterator_value`.
4. **(If slice 2 shows no GC dependency, i.e. a pure logic bug.)** Keep the
   large-iteration stress reproduction from slice 1 in `tests/` (per this
   project's rule that allocation-volume-dependent stress checks live in
   `tests/`, not `test262-extra/`), and additionally distill the *exact*
   incorrect state transition (e.g. the `await_tmp` sentinel being clobbered
   or misread across the loop body's own await) into a small, deterministic,
   GC-independent test262-extra case under
   `test262-extra/language/statements/for-await-of/` that fails every run
   with no reliance on allocation volume, then fix `eval.rs`'s
   `ForOfHead`/`async_fn_suspend_at_await` handling (and/or
   `generator_transform.rs`'s lowering) so both the distilled case and the
   `tests/` stress case go green.
5. **Regression sweep.** Once either branch's fix lands, re-run the original
   issue repro at both configurations recorded in this plan's diagnosis
   (default engine, `runIteration` × 6; `--bytecode`, `runIteration` × 1)
   and confirm both now complete with `D`/`D5` printed and no rejection —
   these, not the issue's original single-iteration repro (already passing
   at HEAD for unrelated reasons), are the acceptance criteria.

## 5. Test surface

- `test262/test/language/statements/for-await-of/` — targeted run; this is
  the construct at fault.
- `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/`,
  `test262/test/built-ins/AsyncGeneratorFunction/`,
  `test262/test/built-ins/AsyncGeneratorPrototype/`,
  `test262/test/built-ins/AsyncFromSyncIteratorPrototype/`,
  `test262/test/built-ins/AsyncIteratorPrototype/`,
  `test262/test/language/statements/async-function/` — targeted run; these
  exercise the same await-suspend/resume machinery
  (`async_fn_suspend_at_await`, `AsyncFunctionState`) the fix touches.
- The allocation-volume-dependent stress reproduction (TDD slice 1, and
  slice 4's fallback) is not test262-conformance material — it belongs in
  `tests/`, run via `cargo test --release`, per this project's rule that
  "exact host-compatibility diagnostics and engine resource-limit or stress
  checks remain in `tests/`."
- If slice 2 confirms a genuine GC-timing dependency, the distilled
  deterministic case belongs in `test262-extra/` (run via
  `uv run python scripts/run-test262.py test262-extra/`) per this project's
  rule that "engine-internal heuristics [...] when the failure changes an
  observable ECMAScript value" get a test262-extra regression — it is
  spec-correctness-observable (a bound value must equal the awaited
  `IteratorResult`'s `[[value]]`) even though test262 itself has no
  allocation-pressure conformance tests.
- Full `uv run python scripts/run-test262.py` (baseline comparison against
  `origin/main:test262-pass.txt`, not rewritten) before opening the PR,
  regardless of which branch the fix lands on.
- `uv run python scripts/run-custom-tests.py` for `tests/`.
- `./scripts/run-jetstream.py --test async-fs` (if the harness names this
  workload distinctly; otherwise the manual repro in this plan's diagnosis)
  as an end-to-end sanity check, not a substitute for the targeted tests
  above.

## 6. Regression risk

- **If the fix touches `gc.rs`'s write-barrier/rooting (`remember_if_old`,
  `gc_write_barrier_value`, `collect_gc_roots`, `object_requires_persistent
  _minor_scan`, or the `AsyncFunctionState`/`for_of_stack` root walk):**
  the failure mode of *under*-rooting is silent data loss (this bug);
  the failure mode of *over*-rooting is correctness-safe but slows minor GC
  toward major-GC-like behavior. The canaries for the latter are the
  long-running Node-compat library harnesses gated by wall-clock budgets:
  `big.js` (~7 min), `uglify-js` (~15 min), `highlight.js` (~30 min) — a
  regression here would show as a timeout, not a wrong answer. Also
  re-run `test262/test/built-ins/FinalizationRegistry/` and
  `test262/test/built-ins/WeakRef/` — these are the test262 directories
  most likely to break if a rooting change stops routine collection from
  ever running (a `FinalizationRegistry` callback that never fires, or a
  `WeakRef` that never clears).
- **If the fix touches `eval.rs`'s `ForOfHead`/`async_fn_suspend_at_await`
  or `generator_transform.rs`'s lowering:** this is exactly the
  tree-walker hot path (`eval_expr`/`exec_statement`) and the area
  `#690`-`#692`/`#688`/`#672`/`#658`/`#604` have been actively hardening;
  re-running the full `for-of`/`for-in`/generator/async-generator test262
  directories (listed above, plus
  `test262/test/language/statements/for/`,
  `test262/test/language/statements/for-in/`,
  `test262/test/language/statements/for-of/`) is the direct regression
  check, since this is shared machinery for every suspending loop, not
  just `for await`.
- **Bytecode fast path:** confirmed not touched by this fix (no suspend/
  resume support exists in `bytecode/vm.rs` today), but re-run
  `cargo test --release` (covers `src/interpreter/bytecode/tests.rs`) to
  confirm the bail-to-tree-walker boundary for `await`/`yield`-bearing
  bodies is unaffected.
- **Baseline:** do not update `test262-pass.txt`; compare against
  `origin/main:test262-pass.txt` as usual. A fix in this area is expected to
  be neutral-to-positive on the baseline (it corrects a silent-corruption
  bug, not a spec-interpretation change), but any newly-passing test should
  still be cross-checked against 262/spec rather than assumed.

## 7. Out of scope

- **The dropped-unhandled-rejection / exit-0/no-stderr usability gap** named
  in the issue body is explicitly a separate concern (the
  `HostPromiseRejectionTracker` host hook, ECMA-262 Promise Abstract
  Operations) and is not part of this fix. Do not implement a
  rejection-reporting/exit-code change under this issue.
- **Hardening `iterator_complete`/`iterator_value` to reject a
  non-`IteratorResult` object** (making a future instance of this class of
  bug throw immediately instead of silently propagating `undefined`) is a
  reasonable defensive follow-up but is not this issue's fix — it treats the
  symptom (silent propagation), not the cause (wrong value delivered), and
  bundling it risks masking whether the real fix actually resolved the
  delivery bug. Track separately if implementation still wants it after
  slice 3/4 lands.
- **Updating the issue's own repro script** to the two configurations this
  plan identified as still-reproducing (see §1) belongs in a `gh issue
  comment`, not in this PR's diff.
- **Any refactor of the generator-transform's `SentValueBinding` naming
  scheme** (e.g. making `"{iter_var}__await"`-style synthetic names
  collision-proof by construction) beyond what slice 1-4's bisection
  requires to fix #679 — do not preemptively redesign the naming/allocation
  scheme for temps across the whole transform in this PR.
- **`run-jetstream.py`/JetStream harness changes** — #681 already closed the
  runner-side gap for this benchmark family; no further `scripts/` changes
  are anticipated here.
