# Plan: issue #788 — async generator `.return()` during a pending `Await`

## 1. Problem restated

The issue claims that calling `.return()` on an async generator while it is
suspended inside an `Await` (not a `yield`) causes the returned promise to
settle on the next few microtask turns instead of staying pending forever,
and that the outer for-of iterable's own iterator never gets `.return()`
called on it even though the loop is abandoned. The issue's own repro,
re-run verbatim on this branch, does **not** reproduce that: jsse currently
matches `node` exactly, both on the issue's literal script (object-pattern
for-of-head default) and on an array-pattern twin — neither promise settles,
and the outer iterator's `returnCalls` stays `0`. The `p1 resolved:
{"done":true}` / `p2 resolved: {"value":"done-value","done":true}` /
`returnCalls: 0` transcript quoted in the issue is in fact what you get when
the head-default `await` never suspends at all (a *different*, since-fixed
defect — #773/#772/#789 landed several "lower await in destructuring
defaults" fixes on this exact branch after #788 was filed) and the loop runs
to normal completion before the queued `return()` is ever serviced: p1
settles via normal step-completion, p2 settles via `AsyncGeneratorAwaitReturn`
on the now-completed generator, and the outer iterator is never touched
because the loop was never interrupted — all spec-correct. A genuinely
interrupted case (resolving the pending `Await` while a `.next()` and a
queued `.return()` are both outstanding, with a real `yield` later in the
loop body to deliver the return at) also matches `node` on this branch, as
does a six-site sweep of other park points (`for await` head, `yield*` into
an async iterator, a catch-param default, a `for`-init, a `finally`, and
`await using` disposal) given a never-settling operand. See `docs/adr/2026-
09-21-2246-async-generator-awaiting-return-parking.md`: jsse deliberately has
no `StateMachineExecutionState` variant for "executing behind a pending
Await" — the spec's own invariant ("the queue is non-empty iff
`[[AsyncGeneratorState]]` is `executing` or `draining-queue`",
spec.html:50604) is implemented by leaving the in-flight request at the head
of `[[AsyncGeneratorQueue]]` until its own continuation fires, which is what
`async_gen_enqueue`'s `!is_executing && queue_len == 1` gate keys off. The
`Executing` match arms in the `.next`/`.return`/`.throw` entry points
(`generator_runtime.rs:2226`, `:2547`, `:3845`, `:6440`, `:6551`, `:806`) are
unreachable by design for exactly this reason (documented in that ADR's
"Known boundaries").

So there is no live defect to fix in production code. What #774 actually
deferred, and what #788 was filed to triage, is **regression coverage**:
nothing in `test262/` or `test262-extra/` currently pins down "a `.return()`
queued behind a pending for-of-head destructuring-default `Await` stays
pending and leaves the outer iterable untouched until the generator next
reaches a real yield or completes" for either pattern shape. That gap is
real and this plan closes it with tests only.

## 2. Spec basis

- `%AsyncGeneratorPrototype%.return ( value )` (spec.html:50527-50546,
  `sec-asyncgenerator-prototype-return`): step 6 — if
  `[[AsyncGeneratorState]]` is `executing` or `draining-queue`, `.return()`
  does nothing beyond `AsyncGeneratorEnqueue` (step 5).
- `AsyncGeneratorEnqueue` (spec.html:50699-50714,
  `sec-asyncgeneratorenqueue`): appends an `AsyncGeneratorRequest` to
  `[[AsyncGeneratorQueue]]`; no other effect.
- Internal slots table (spec.html:50579-50613,
  `sec-properties-of-asyncgenerator-intances`): `[[AsyncGeneratorQueue]]`
  "is non-empty if and only if `[[AsyncGeneratorState]]` is either
  `executing` or `draining-queue`" (except during state transitions) — the
  invariant jsse's queue-head-stays-parked design relies on.
- `Await` (spec.html:51047-51081, `await`): never reads or writes
  `[[AsyncGeneratorState]]`. Suspending at an `Await` inside a generator body
  leaves the generator's own state at whatever it already was
  (`executing`), which is why `.return()` called during that window must be
  treated as step 6's do-nothing branch, not step 5's `suspended-yield`
  resume branch.
- `AsyncGeneratorYield` (spec.html:50789-50821,
  `sec-asyncgeneratoryield`): step 6-8 — on reaching a real `yield`,
  `AsyncGeneratorCompleteStep`s the front (currently-running) request, then
  if the queue is still non-empty, delivers the *next* queued request's
  completion (e.g. the parked `return()`) back into the body without
  truly suspending — this is the "interrupt at the next yield" path the
  second new test locks in.
- `AsyncGeneratorAwaitReturn` / `AsyncGeneratorDrainQueue`
  (spec.html:50823-50890): the completed-generator settlement path exercised
  by the non-interrupted (empty loop body) variant, already covered by
  existing test262 (`AsyncGeneratorPrototype/return/return-state-completed*.js`)
  and test262-extra (`async-generator-await-return-parks-later-requests.js`)
  — not re-covered here, since that ADR and those tests are about
  `AsyncGeneratorAwaitReturn`'s *own* `Await(value)`, not the generic
  body-level `Await` this issue is about.

No JavaScript syntax or semantics are being changed by this plan — the
investigation found existing behavior already spec-conformant. The clauses
above ground the *tests* being added, per the planning brief's requirement
that test-only work still be grounded in the spec clauses it exercises.

## 3. Files to touch

- `test262-extra/async-generator-return-queued-behind-head-default-await-stays-pending.js`
  (new)
- `test262-extra/async-generator-return-queued-behind-head-default-await-interrupts-at-yield.js`
  (new)
- No `src/` changes. No `docs/adr/` addition — this doesn't introduce a new
  architectural decision, it confirms and pins down an existing one that's
  already documented in `docs/adr/2026-09-21-2246-async-generator-awaiting-
  return-parking.md`. No `CONTEXT.md` change — no new vocabulary.

## 4. TDD slices

Both slices are expected to be **green immediately** against the current
branch — there is no production defect to turn red-to-green on. The
"test-first" discipline here is: write the test from the spec clauses above
(not from today's observed output), run it, and confirm it passes for the
reason the spec requires (the `queue_len` gate in `async_gen_enqueue`), not
by accident. If either slice is unexpectedly red, that *is* a live defect —
stop and re-triage rather than adjusting the test to match.

1. **Slice 1 — queued `.return()` stays pending, outer iterator untouched,
   until natural completion.**
   File: `test262-extra/async-generator-return-queued-behind-head-default-
   await-stays-pending.js`.
   Behavior under test: for both `for (let { b = await deferred } of outer)
   {}` (object pattern) and `for (let [a, b = await deferred] of outer) {}`
   (array pattern), with `outer` a custom-iterable counting its own
   `.return()` calls and `deferred` an externally-resolvable promise:
   - `gen.next()` then `gen.return(x)` called back-to-back, synchronously.
   - Across several microtask ticks *before* `deferred` resolves: neither
     promise has settled, and `outer`'s `.return()` has not been called.
   - After `deferred` resolves (with the loop body empty, so no `yield` to
     interrupt at): the `.next()` promise settles per
     `AsyncGeneratorCompleteStep`/fall-through-completion, the `.return()`
     promise settles per `AsyncGeneratorAwaitReturn`, and `outer`'s
     `.return()` is still never called (the loop was never abruptly exited).
   Production code: none — this slice is the regression anchor for the
   issue's literal repro (both pattern shapes) and should pass unmodified.

2. **Slice 2 — queued `.return()` is delivered at the next real `yield`,
   closing the outer iterator exactly once.**
   File: `test262-extra/async-generator-return-queued-behind-head-default-
   await-interrupts-at-yield.js`.
   Behavior under test: same shape as slice 1, but the loop body is `{
   yield b; }`. Sequence: `gen.next()` suspends at the head-default `Await`;
   `gen.return(x)` is enqueued behind it; resolving `deferred` lets the body
   reach the `yield`, which (per `AsyncGeneratorYield` steps 6-8) settles the
   `.next()` promise with `{value: deferred-value, done: false}` and
   delivers the queued return completion back into the body without
   suspending, propagating an abrupt `Return` through the `for`-of's
   `IteratorClose` — so `outer.returnCalls()` becomes exactly `1`, and
   `gen.return(x)`'s own promise settles with `{value: x, done: true}`.
   Covers both object and array pattern shapes in the same file (mirroring
   `async-generator-destructuring-default-await.js`'s existing structure).
   Production code: none — validated against this branch's binary and
   against `node` (both agree) before writing this plan.

## 5. Test surface

- Targeted test262 run to confirm no overlap/regression in the neighboring
  suite: `uv run python scripts/run-test262.py
  test262/test/built-ins/AsyncGeneratorPrototype/return/` and
  `test262/test/built-ins/AsyncGeneratorPrototype/next/` (these already
  contain `request-queue-order-state-executing.js` and
  `request-queue-await-order.js`, which cover the *synchronous-executing*
  and *AsyncGeneratorAwaitReturn's-own-Await* cases respectively, not the
  generic body-`Await` case this issue is about — confirmed by reading both
  files; no duplication).
- New coverage lives in `test262-extra/` (per `CLAUDE.md`'s rule: spec-correct
  behavior not covered by test262, following test262 file conventions,
  naming the spec clause under test). Run via `uv run python
  scripts/run-test262.py test262-extra/async-generator-return-queued-behind-
  head-default-await-stays-pending.js test262-extra/async-generator-return-
  queued-behind-head-default-await-interrupts-at-yield.js`.
- Full custom suite as a sanity pass: `uv run python
  scripts/run-custom-tests.py`.
- `cargo test --release` is not expected to be affected (no `src/` changes)
  but should be run anyway per the quality gate.
- Do **not** run the full `test262/` suite expecting a baseline move — this
  plan adds no engine behavior, so `test262-pass.txt` (read from
  `origin/main`) is irrelevant here and must not be touched.

## 6. Regression risk

None from this plan directly (no `src/` change). The two new tests do,
however, become a tripwire for *future* regressions in the machinery this
investigation exercised:
- `async_gen_enqueue`'s `!is_executing && queue_len == 1` gate
  (`generator_runtime.rs:2811`) — the mechanism that currently makes the
  `Executing` match arms unreachable. If a future change pops the queue head
  before a parked request's own continuation fires (the exact bug ADR-2026-
  09-21-2246 fixed for `AsyncGeneratorAwaitReturn` under #712), slice 1 would
  go red.
  - `async_gen_suspend_at_await` (`generator_runtime.rs:5825`) and
  `AsyncGeneratorYield`'s queued-request delivery path (the driver's
  handling of a non-empty queue on reaching a real yield) — if a future
  change delivers the queued return too early (before the yield) or not at
  all (never interrupting the loop), slice 2 would go red.
- Both slices exercise the for-of-head destructuring-default lowering
  (`generator_transform.rs` / the array-pattern and object-pattern binding
  paths) that #772/#773/#774/#789 touched most recently — they're a useful
  guard against that lowering regressing the suspension point itself.
- No interaction with the bytecode fast path (generator/async-generator
  bodies run through the tree-walker's state-machine driver regardless), GC
  rooting (no new root-scope shape introduced), or the Node-compat library
  harnesses.

## 7. Out of scope

- Any change to `StateMachineExecutionState` or the generic
  `async_gen_enqueue`/`async_gen_process_queue` machinery — deliberately
  rejected per ADR-2026-09-21-2246; there is no observable behavior it would
  change, only exhaustive-match churn.
- Investigating or fixing the *already-fixed* "await in a destructuring
  default doesn't suspend at all" defect the issue's transcript was actually
  evidence of — that's `git log`-verifiable as resolved by #772/#773/#789 and
  not re-opened by anything here.
- Removing or marking dead the unreachable `Executing` match arms in the
  `.next`/`.return`/`.throw` entry points — already tracked as a known,
  deliberate boundary in ADR-2026-09-21-2246; not this issue's scope.
- Broader sweep-site coverage (the six additional park points probed during
  triage — `for await` head, `yield*` into an async iterator, catch-param
  default, `for`-init, `finally`, `await using` disposal — all of which
  already match `node`): no divergence found, so no new test is planned for
  them under this issue. A future issue can add targeted coverage for any of
  those sites individually if desired.
