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
- **Ruled out the outer loop's own machinery.** Wrapped
  `randomFileContents()` in a pass-through async iterable
  (`instrumented(inner)`) whose own `next()` does `const r = await
  it.next(); if (r === undefined || r.value === undefined) print("SOURCE
  undefined at n=" + n);` before returning `r`, and consumed it via `for
  await (const fileContents of instrumented(randomFileContents()))`. Under
  `--bytecode`, `SOURCE undefined at n=448` and `n=758` printed — i.e. the
  corruption is already present in `r` **before** it is even handed back to
  the outer `for await`'s own head logic. This rules out
  `StateTerminator::ForOfHead`'s await/resume branch in `eval.rs`
  (~9166-9224) and `async_fn_suspend_at_await`/`AsyncFunctionState`
  (`eval.rs:9466`, `scheduler.rs:269-283`) as the fault: those only see
  `randomFileContents()`'s `.next()` result *after* it is already wrong.
  The fault is inside the async generator's *own* yield/await delivery.
- **Localized to a structural hazard in that delivery path.** The async
  generator's own state-machine driver lives in
  `src/interpreter/eval/generator_runtime.rs` (`generator_next_state_machine`
  and friends handle sync generators; `async_generator_next_state_machine_impl`
  (~3267), its `StateTerminator::Await` branch (~5815-5910),
  `apply_sent_value_binding` (~5917), and `async_gen_await_resume` (~5935)
  handle async generators — a separate driver from `eval.rs`'s
  `async_function_resume`, confirmed by `eval.rs:9326`'s
  `StateTerminator::Yield => unreachable!("Yield terminator in async
  function")`, which only holds for plain async functions). Per
  `AsyncGeneratorYield` (`spec/spec.html#sec-asyncgeneratoryield`), every
  `yield` in an async generator involves its
  *own* internal `Await`, so `randomFileContents`'s single `yield new
  DataView(result)` per turn is itself one full suspend/resume cycle through
  this driver.

  `async_gen_process_queue` (`generator_runtime.rs:2552`) implements
  `AsyncGeneratorDrainQueue`: it resets a flag
  (`self.scheduler.set_async_gen_yield_pending(false)`, line 2569), steps the
  generator's state machine once, then — the comment at line 2606-2607 is
  explicit — reads that *same* flag to decide whether the step actually
  suspended on a pending promise (leave the request queued; a fulfill/reject
  handler will resume it later) or ran to completion synchronously (pop the
  request and immediately drain the next one, recursively). That flag,
  `async_gen_yield_pending` (`scheduler.rs:192`), is declared as a single
  **`bool` on the whole `JobScheduler`** — not keyed by generator id, unlike
  the adjacent, correctly-`u64`-keyed `async_gen_queues:
  FxHashMap<u64, VecDeque<AsyncGenRequest>>` one field above it. It is
  written to `true` from at least seven sites in `generator_runtime.rs`
  (`3008`, `4591`, `4689`, `4722`, `4771`, `4999`, `5909`) across the
  `Yield`/`Await`/`yield*`-delegation terminators, and read back only twice
  (`2608`, `5992`), always as "did *the* generator I just stepped suspend."
  If any nested/re-entrant activity for a *different* async-generator
  instance (or a different suspension path of the same one) touches this
  flag while a `async_gen_process_queue`/`async_generator_next_state_machine
  _*` call for the *current* generator is still on the Rust call stack, the
  read at line 2608 answers the wrong question: a generator that actually
  finished this step synchronously with a valid `result` gets treated as "it
  suspended, a callback will finish it" (`result` discarded, line 2610) —
  or a generator that genuinely suspended gets treated as "already
  finished" and its request popped/drained before the real value arrives.
  Either misreading is silent (no exception at the corruption site) and
  would surface exactly as observed: intermittent, allocation/concurrency-
  volume correlated (more concurrently in-flight async-generator activity →
  more chances for the flag to be touched by the wrong generator's step
  before the outer read), with no fixed, deterministic trigger count. This
  matches every symptom collected in this plan's diagnosis better than any
  GC-rooting theory: it needs no object to be collected, no id to be
  recycled, and no value to be literally lost — only one bit of *shared*
  bookkeeping to be read for the wrong generator.

  **This is a structural hazard found by static reading of
  `async_gen_yield_pending`'s single-`bool`, unkeyed shape next to a
  correctly-keyed sibling field — it has not yet been dynamically proven to
  be the trigger inside `async-file-system.js` specifically.** Confirming
  that (and finding what, concretely, re-enters while a step is in flight —
  candidates: `yield*`-delegation resuming a *different* generator inline,
  a `.then()` reaction for one generator's earlier await firing during
  another's synchronous step, or microtask draining triggered from inside
  `call_function`) is TDD slice 1's job, not this plan's.

This is as far as a planning stage should go without writing code.

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
  #sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`):
  for `iteratorKind` = ~async~, each iteration must bind the loop variable to
  `IteratorValue(?Await(IteratorNext(iteratorRecord)))` — the `[[value]]` of
  *that* turn's own iterator-result, never a stale or unrelated value.
- **AsyncGeneratorYield ( value )** (`spec/spec.html
  #sec-asyncgeneratoryield`): each `yield` in an async generator body awaits
  its value, then resumes the *specific* suspended generator that issued it
  with the *specific* completion produced for it — never a different
  generator's completion, and never an already-consumed one. This is the
  clause the diagnosis in §1 points at: the engine's own bookkeeping for
  "did the generator I just stepped suspend or finish" must be scoped per
  generator, or two async generators' suspend/resume cycles can answer each
  other's question.
- **Await ( value )** (`spec/spec.html#await`, under Async Function Abstract
  Operations): the execution context must be suspended and later resumed
  with exactly the value the awaited promise settled with — never another
  execution context's settled value.

## 3. Files to touch

Engine (all under `src/interpreter/`):

- `scheduler.rs` — `async_gen_yield_pending: bool` field (~192) and its
  accessors `set_async_gen_yield_pending`/`is_async_gen_yield_pending`
  (~255-260). Primary fix site: re-scope this to per-generator state (e.g.
  fold it into the existing per-id `async_gen_queues: FxHashMap<u64, ...>`
  entry, or thread it as a return value / explicit parameter instead of
  shared mutable scheduler state) once slice 1 confirms the cross-generator
  read is real.
- `eval/generator_runtime.rs` — every write site of the flag
  (`async_gen_process_queue` ~2552-2623, and the `Yield`/`Await`/`yield*`
  terminator arms that set it to `true`: ~3008, ~4591, ~4689, ~4722, ~4771,
  ~4999, ~5909) and its other read site (~5992, inside
  `async_generator_return_state_machine_with_promise` or a neighboring
  function — confirm exact owner when editing). Every call site that
  currently threads state through this flag needs to thread it through
  whatever per-generator replacement slice 1's fix uses instead.
- `builtins/iterators.rs` — `iterator_complete`/`iterator_value`
  (~4954-4973) are candidates for tightening (rejecting a non-`IteratorResult`
  rather than silently defaulting to `false`/`undefined`) as a defense in
  depth once the upstream defect is fixed, but this alone only turns the
  symptom into a `TypeError` at the right place — not this issue's fix by
  itself (see Out of scope).
- `eval.rs` — `StateTerminator::ForOfHead` (~9122-9224) and
  `async_fn_suspend_at_await` (~9466) were the *original* suspects in this
  plan's diagnosis and were ruled out by the `instrumented()`-wrapper probe
  in §1 (the corruption is already present in the async generator's own
  `.next()` result, before this code ever sees it). Do not touch these
  unless slice 1's dynamic confirmation contradicts the static finding and
  points back here.

Non-engine:

- None required. This is not a `scripts/`/CI/benchmark-harness gap; #681
  already closed the runner-side "no JSON output" misclassification for this
  family. No `docs/adr/` entry unless implementation lands on a real
  architectural redesign of the async-generator drain-queue protocol (see
  Out of scope) rather than re-scoping one field.

## 4. TDD slices

1. **Dynamically confirm the cross-generator read, with a named failing
   test.** Add `tests/async-generator-concurrent-yield-pending-flag.js` (a
   plain script, run via `uv run python scripts/run-custom-tests.py
   tests/async-generator-concurrent-yield-pending-flag.js` — pass = exit 0,
   fail = a thrown/uncaught error per that runner's convention; see other
   `tests/*.js` files for the header/assertion style already used there).
   Drive **two** async generators concurrently, engineered to make one's
   suspend/resume cycle land while the other's `async_gen_process_queue`
   step is still executing — e.g. two `for await` loops over two separate
   `async function*` sources, advanced by interleaving `.next()` calls
   inside a shared `Promise.all`/microtask-interleaved driver (mirroring
   how `randomFileContents()`'s consumption in `setupDirectory()` overlaps
   with the rest of `Benchmark`'s concurrent async bookkeeping), asserting
   neither generator's yielded value is ever lost across many iterations.
   This is expected to be **red** at HEAD. If it does *not* go red, the
   `async_gen_yield_pending` cross-talk in §1 is not the (or not the only)
   trigger — fall back to reproducing directly against a scratch copy of
   `/tmp/JetStream/generators/async-file-system.js` using the same
   `instrumented()`-wrapper probe already built in this plan's diagnosis,
   identify what *does* re-enter while a step is on the stack, and update
   this slice before writing any fix.
2. **Fix the scoping.** Once slice 1 is red for a known reason, re-scope
   `async_gen_yield_pending` (and any other bookkeeping the same call path
   uses this way) so a step for generator A cannot be answered by a signal
   generator B produced, and make slice 1 green. Do not weaken
   `iterator_complete`/`iterator_value` as the fix — they may still be
   tightened separately (see Out of scope) but that must not be how slice 1
   passes.
3. **Distilled deterministic regression.** Once the mechanism is confirmed
   and fixed, reduce slice 1's two-generator interleaving to the smallest
   deterministic case that still exercises the same code path (no reliance
   on allocation volume — this is a scheduling/re-entrancy bug, not a GC
   one, so it should not need thousands of iterations to force). Because it
   changes an observable ECMAScript value (which generator's yielded value
   a `for await` loop variable binds to) rather than being an allocation- or
   resource-limit stress check, this belongs in `test262-extra/` per this
   project's rule (`CLAUDE.md`), not `tests/`:
   `test262-extra/language/statements/for-await-of/concurrent-async-generators-do-not-cross-deliver-yielded-values.js`,
   following existing test262 file header conventions (`esid`, `description`,
   `info` citing `AsyncGeneratorYield` and `ForIn/OfBodyEvaluation`,
   `flags: [async]`, and the `$262`/`print`/`doneprintHandle.js` patterns
   already used elsewhere in this repo's `test262-extra/`). Keep slice 1's
   `tests/` file too — it is the closer analogue of the actual JetStream
   trigger and a cheap regression net even after the distilled case exists.
4. **Regression sweep.** Re-run the original issue repro at both
   configurations recorded in this plan's diagnosis (default engine,
   `runIteration` × 6; `--bytecode`, `runIteration` × 1) and confirm both
   now complete with `D`/`D5` printed and no rejection — these, not the
   issue's original single-iteration repro (already passing at HEAD for
   unrelated reasons), are the acceptance criteria.

## 5. Test surface

- `test262/test/language/statements/for-await-of/`,
  `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/`,
  `test262/test/built-ins/AsyncGeneratorFunction/`,
  `test262/test/built-ins/AsyncGeneratorPrototype/`,
  `test262/test/built-ins/AsyncFromSyncIteratorPrototype/`,
  `test262/test/built-ins/AsyncIteratorPrototype/` — targeted run; these
  exercise `AsyncGeneratorEnqueue`/`AsyncGeneratorDrainQueue` and the
  `async_gen_process_queue`/`async_gen_yield_pending` machinery directly.
  None of these are individually likely to catch *this* bug today (they
  each drive a single generator, not concurrent ones), which is exactly why
  a new test262-extra case is needed — but they are the direct regression
  surface for any change to `async_gen_process_queue`'s control flow.
- The allocation/interleaving-dependent reproduction (TDD slice 1) is not
  test262-conformance material — it belongs in `tests/`, run via
  `uv run python scripts/run-custom-tests.py`, per this project's rule that
  "exact host-compatibility diagnostics and engine resource-limit or stress
  checks remain in `tests/`." (Re-check this categorization if slice 1 turns
  out to still need real allocation volume rather than pure interleaving —
  see the note in §4 slice 1.)
- The distilled deterministic case (slice 3) belongs in `test262-extra/`
  (run via `uv run python scripts/run-test262.py test262-extra/`) per this
  project's rule that "engine-internal heuristics [...] when the failure
  changes an observable ECMAScript value" get a test262-extra regression.
- Full `uv run python scripts/run-test262.py` (baseline comparison against
  `origin/main:test262-pass.txt`, not rewritten) before opening the PR.
- `uv run python scripts/run-custom-tests.py` for `tests/`.
- The manual repro built in this plan's diagnosis (§1) as an end-to-end
  sanity check against the real JetStream benchmark, not a substitute for
  the targeted tests above.

## 6. Regression risk

- **Primary risk area: `async_gen_process_queue` and
  `AsyncGeneratorDrainQueue`'s recursive drain
  (`eval/generator_runtime.rs:2552-2623`).** This function already
  recurses into itself (line 2620) to drain queued requests once a step
  settles synchronously; re-scoping the suspend/finish signal touches every
  call site that currently relies on the flag's *global* value implicitly
  agreeing across nested calls. Re-run every async-generator test262
  directory listed in §5, plus anything in this repo that exercises
  multiple concurrently-live async generators (`for await` inside `Promise
  .all`, `yield*` delegating into another async generator) — those are
  exactly the shapes that would have been silently relying on (or silently
  broken by) the current unscoped flag.
- **`gc.rs` is very unlikely to need changes.** The diagnosis in §1 found no
  evidence of a rooting/write-barrier gap (the `Reflect.get`/`hasOwnProperty`
  probe ruled out object-arena id recycling, and the localization points at
  scheduler bookkeeping, not memory management) — do not touch
  `remember_if_old`/`gc_write_barrier_value`/`collect_gc_roots` speculatively
  under this issue. If slice 1's dynamic confirmation contradicts this and
  a real rooting gap turns up instead, that changes the write-barrier
  regression profile: *under*-rooting is silent data loss (failure mode
  matching this bug), *over*-rooting is correctness-safe but slows minor GC
  toward major-GC-like behavior, and the canaries for the latter are the
  long-running Node-compat library harnesses (`big.js` ~7 min, `uglify-js`
  ~15 min, `highlight.js` ~30 min — a regression would show as a timeout,
  not a wrong answer) plus `test262/test/built-ins/FinalizationRegistry/`
  and `test262/test/built-ins/WeakRef/` (most likely to break if routine
  collection stops running).
- **Bytecode fast path:** not touched by this fix (no suspend/resume
  support exists in `bytecode/vm.rs` today; `--bytecode` only reproduces
  this bug faster by raising allocation/scheduling pressure per
  wall-clock iteration, it does not host the defect). Re-run
  `cargo test --release` (covers `src/interpreter/bytecode/tests.rs`) to
  confirm the bail-to-tree-walker boundary for `await`/`yield`-bearing
  bodies is unaffected regardless.
- **Baseline:** do not update `test262-pass.txt`; compare against
  `origin/main:test262-pass.txt` as usual. A fix in this area is expected to
  be neutral-to-positive on the baseline (it corrects a silent-corruption
  bug, not a spec-interpretation change), but any newly-passing test should
  still be cross-checked against spec/test262 rather than assumed.

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
  symptom (silent propagation), not the cause (a scheduler-scoping bug), and
  bundling it risks masking whether the real fix actually resolved the
  cross-generator delivery bug. Track separately if implementation still
  wants it after slice 2 lands.
- **Any broader redesign of the `AsyncGeneratorRequest` queue/drain
  protocol** beyond re-scoping the one flag slice 1/2 identifies — do not
  preemptively rewrite `async_gen_process_queue`'s recursion or the queue's
  data structures in this PR.
- **`eval.rs`'s `ForOfHead`/`async_fn_suspend_at_await`** — investigated and
  ruled out in this plan's diagnosis (§1); do not refactor this code under
  #679 absent new evidence.
- **Updating the issue's own repro script** to the two configurations this
  plan identified as still-reproducing (see §1) belongs in a `gh issue
  comment`, not in this PR's diff.
- **`run-jetstream.py`/JetStream harness changes** — #681 already closed the
  runner-side gap for this benchmark family; no further `scripts/` changes
  are anticipated here.
