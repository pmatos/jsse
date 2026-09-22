# Plan: issue #719 — pending exception/return consumed by the wrong `TryExit`

## 1. Problem restated

All three state-machine drivers (plain async functions in `eval.rs`, sync
generators and async generators in `generator_runtime.rs`) store a return or
throw completion intercepted by a running `finally` at *driver* or *iterator*
scope (`AsyncFunctionState::pending_return`/`saved_finally_exception`,
`IteratorState::{StateMachineGenerator,StateMachineAsyncGenerator}::pending_exception`/`pending_return`)
instead of on the specific `TryContextInfo` whose `finally` owns it. Because
every `TryExit` — including one belonging to an unrelated, more deeply nested
`try`/`finally` entered *inside* that finally's own body — checks the same
unscoped slot, a nested `TryExit`, a suspension, or a `yield*` delegation can
rethrow, re-return, overwrite, or silently drop an outer finalizer's pending
completion before the outer finalizer's own remaining statements or its own
`TryExit` ever run. `break`/`continue` already avoids this: PR #717 parks a
pending jump on `TryContextInfo::pending_loop_control`, scoped to the exact
context whose finalizer is running it, and that mechanism does not have this
bug. This issue generalizes the same ownership model to `return` and `throw`.

## 2. Spec basis

- **`sec-try-statement-runtime-semantics-evaluation`** (`spec/spec.html:23182`).
  This is the governing clause. For `try Block Finally`: `F` is the Finally
  clause's own completion; `1. If F is a normal completion, set F to B`
  (`B`/`C` being whatever completion entered the finally) — i.e. a *normal*
  finally completion restores the completion that entered it, while an
  *abrupt* `F` (a new throw, a new return, or loop control escaping the
  finally) replaces it outright. This is the exact rule the bug violates: the
  engine's current shared-slot storage lets an unrelated nested finally's own
  normal-or-abrupt completion "restore" or "replace" the *outer* finally's
  entering completion, rather than each `try`/`finally`'s own `F`/`C`.
- **`sec-generatorresume`** / **`sec-generatorresumeabrupt`**
  (`spec/spec.html:50340`, `:50365`) and **`sec-asyncgeneratorresume`**
  (`:50749`): resuming a suspended generator "resumes the suspended evaluation
  ... using [the] Completion Record" — i.e. a generator is specified as a real
  suspended execution of nested statements, so a completion delivered at one
  `yield` must propagate through exactly the same nested Try Statement
  Evaluation as an ordinary (non-generator) function body would. The engine's
  driver is a flattened re-implementation of that nested evaluation; the
  `try_stack` is what stands in for the spec's implicit nested execution
  contexts, and it must preserve the same nesting-scoped ownership.
- **`sec-generator-function-definitions-runtime-semantics-evaluation`**,
  clause for `YieldExpression : yield * AssignmentExpression`
  (`spec/spec.html:24279`, esp. steps as `received` becomes a return
  completion at `:24317-24333`): a `yield*` loop receives whatever completion
  resumed it (`received`), forwards it into the delegate's `next`/`throw`/`return`
  method, and when the delegate finishes, *re-produces* a completion of the
  same kind (`ReturnCompletion(_returnedValue_)`) for the enclosing code to
  keep routing. The delegation loop is required to carry an in-flight return
  completion through to its own exit, not discard it — this is scenario 4 in
  the issue (pending return lost across `yield*`).
- **`sec-asyncgeneratoryield`** / **`sec-asyncgeneratorunwrapyieldresumption`**
  (`spec/spec.html:50789`, `:50772`): each `yield` in an async generator is
  its own suspend/resume step that unwraps exactly the completion delivered
  *for that step*. A throw parked behind a `finally` that contains two
  `yield`s must not surface at the first `yield`'s resume — it is not "the
  completion resuming this step," it is a completion still in flight through
  the try stack. This is scenario 3 (async-generator throw delivered too
  early).

No new JavaScript syntax or semantics are introduced; this is a conformance
fix to bring an already-specified control-flow rule (Try Statement Evaluation)
into effect uniformly across suspension, nesting, and delegation.

## 3. Files to touch

Engine (`src/`):
- `src/interpreter/types.rs` — new `PendingCompletion` enum; replace
  `TryContextInfo::pending_loop_control: Option<LoopControlTarget>` with
  `pending_completion: Option<PendingCompletion>`; remove
  `AsyncFunctionState::{pending_return, pending_loop_control, saved_finally_exception}`;
  remove `IteratorState::{StateMachineGenerator,StateMachineAsyncGenerator}::{pending_exception, pending_return}`
  and update `completed_state_machine_generator`/`completed_state_machine_async_generator`
  and their unit tests (`types.rs:1521-1648`) accordingly.
- `src/interpreter/eval.rs` — `async_function_resume` (`:8184-9737`): the
  `route_return!` (`:8456-8517`) and `route_loop_control!` (`:8524-8591`)
  macros, the exception-routing block (`:8761-8901`), the `TryEnter`/`TryExit`/`EnterFinally`
  match arms (`:9170-9246`), every `AsyncFunctionState` struct literal
  (`:8146-8164`, `:8279-8299`), and `async_fn_suspend_at_await`
  (`:9719-9737`, called from ~9 sites) — its `pending_return`,
  `pending_loop_control`, `saved_finally_exception` parameters are dropped
  since `try_stack` alone carries everything once completions are
  context-owned.
- `src/interpreter/eval/generator_runtime.rs` — the sync driver's
  `TryEnter`/`TryExit`/`EnterFinally` (`:1450-1535`), `route_generator_exception`
  (`:5935-5994`), `route_generator_loop_control` (`:6005-6055`),
  `generator_return_state_machine` (`:1858-2168`), `generator_throw_state_machine`
  (`:2170-2455`); the async driver's `TryEnter`/`TryExit`/`EnterFinally`
  (`:4517-4582`), `check_abrupt_on_resume` and its call sites (`:3414-3421`,
  `:3539-3661`, plus re-arm sites at `:3595`, `:3621`, `:3761`, `:4362`,
  `:4543`, `:4800`, `:5038`), `async_generator_throw_state_machine_with_promise`/return
  equivalents (`:5653-5832`); `yield*` delegation on both drivers — sync
  suspend literals around `:705-756`, `:1257-1303`, async
  `yield_star_suspend_on_inner_result` (`:2699-2718`) and its resume/return
  helpers (`:2740-3139`); every remaining `IteratorState` struct literal in
  this file that sets `pending_exception`/`pending_return` (~20+ sites, found
  via `grep -n "pending_exception\|pending_return"`); `stays_inside_running_finally`
  (`:12-17`) — reused or folded into the generalized routing, not deleted
  outright unless routing subsumes it (test262-extra's existing
  `generator-loop-control-through-suspending-finalizer.js` is the regression
  guard either way).
- `src/interpreter/gc.rs` — `collect_iterator_state_roots`
  (`:1079-1135`, `StateMachineGenerator`/`StateMachineAsyncGenerator` arm
  `:1104-1132`): add tracing of each `try_stack` entry's `pending_completion`
  payload (today this arm elides `try_stack` via `..` — `try_stack` is
  currently untraced everywhere and it is *only* safe because
  `TryContextInfo` carries no `JsValue` yet). `collect_gc_roots`'s
  `iter_async_function_states` loop (`:468-485`): remove the now-dead
  `afs.pending_return`/`afs.saved_finally_exception` tracing arms and add
  tracing of `afs.try_stack`'s `pending_completion` payloads instead.
- `src/interpreter/generator_transform.rs` — expected to need no functional
  change (the issue's own provenance note: "the generated state graph is
  already structurally nested correctly"); touch only if `PendingCompletion`
  is colocated with `LoopControlTarget` here instead of in `types.rs`, or to
  update the doc comment on `LoopControlTarget` if it starts sharing a slot
  with `PendingCompletion::LoopControl`.

Docs:
- `CONTEXT.md` — update the **Loop Control** entry (`:104-106`): it currently
  says the generator drivers park a jump on `TryContextInfo.pending_loop_control`;
  update to describe `pending_completion`/`PendingCompletion` and that
  `Return`/`Throw` now share the same per-context ownership model, not just
  loop control. Cross-reference from a new entry (see below).
- `docs/adr/YYYY-MM-DD-HHMM-pending-completion-ownership.md` — new ADR (timestamp
  set by the implementer at authoring time per `docs/adr/README.md`) recording
  the tagged-`enum` decision (one Completion Record per context, not three
  optional fields) and the "no unification of the three drivers" boundary,
  mirroring the style of `docs/adr/2026-09-21-2300-yield-star-delegated-step-suspension.md`.

Non-engine: none. This issue has no `scripts/`, `benchmarks/`, or `.github/`
component.

## 3a. Design notes: three completion channels, not one

Research surfaced two subtleties the "park on the context" description in §3
glosses over. Both must hold for every driver, or the fix is wrong in a way
that compiles and passes the issue's simplest repro while still failing a
sibling case.

**Catch consumes; only finally parks.** `route_generator_exception`
(`generator_runtime.rs:5935-5994`) and the equivalent block in `eval.rs`
(`:8761-8901`) select *either* a `catch_state` *or* a `finally_state` as the
handler and, today, write the exception into the same unscoped slot either
way. A caught throw is **consumed** by `EnterCatch` to bind the catch
parameter (`generator_runtime.rs:1513`, `:4560`; `eval.rs:9222`) — it must
never be parked on a `TryContextInfo`, because there is no later `TryExit`
that should "restore" it (the catch already ran; the try/catch's own
completion afterward is whatever the catch body produces). Only a
finally-bound selection parks: `try_stack[depth].pending_completion = Some(PendingCompletion::Throw(exc))`
(or `Return`). Every slice that touches routing must keep this branch: catch
→ stays in the in-flight local for `EnterCatch`; finally → moves to the
context.

**Three lifetimes, three homes, not two.** The redesign needs to keep all
three of these distinct, where today's code has collapsed the last two into
one unscoped slot:
1. *In-flight, being routed* — a throw/return just produced by a terminator,
   not yet matched to a handler. Lives in the driver's existing local
   (`pending_exception`/`pending_return` in `eval.rs:8305-8307`,
   `generator_runtime.rs:831-832`/`:3416-3417`). This local already never
   survives a suspension today (routing always resolves it within the same
   loop iteration before any `Yield`/`Await` dispatches), so it needs no
   design change — keep it exactly as a transient local, for both the
   catch-delivery case above and for driving the routing search itself.
2. *Parked behind a running finally* — owned by `TryContextInfo::pending_completion`,
   per §3. Read back only by that context's own `TryExit`.
3. *A fresh resume input for this specific call* — an external
   `.throw(e)`/`.return(v)`, or a rejected/fulfilled await — arriving at a
   driver entry point (`generator_throw_state_machine`, `generator_return_state_machine`,
   `async_generator_throw_state_machine_with_promise`, the equivalent
   `_return_` function, and `async_function_resume`'s existing `sent_value`/`is_error`
   parameters). **This must become an explicit function parameter carried
   into the `_impl` dispatch function, not smuggled through the object's
   persisted `IteratorState`/`AsyncFunctionState` fields.** Today it *is*
   smuggled that way — e.g. `async_generator_throw_state_machine_with_promise`
   writes `pending_exception: Some(exception)` directly onto the object
   (`generator_runtime.rs:5809-5822`) before calling into the impl loop,
   which reads it back as `stored_pending_exception` (`:3198`, seeding the
   local at `:3416`). That round trip through the object is exactly the
   field this plan deletes (it's indistinguishable, once written, from a
   completion parked behind a finally — this *is* the mechanism behind
   scenario 3). `eval.rs::async_function_resume` already does this
   correctly today — `sent_value`/`is_error` are plain call parameters
   (`:8184-8189`) that seed the local `pending_exception` once at `:8272-8276`,
   never round-tripping through a persisted field — so it is the model to
   follow for the generator entry points, not a file that needs new
   plumbing itself.

**`check_abrupt_on_resume`'s "trampoline" job needs its own answer, not just
its "is this fresh" job.** The flag is set from a fresh resume input at
`generator_runtime.rs:3414-3421`, but it is *also* reused internally as a
"go re-check routing at the top of the loop" signal — e.g. `TryExit` doing
`pending_return = Some(ret_val); check_abrupt_on_resume = true; continue;`
(`:4541-4546`), and re-arm sites at `:3595`, `:3621`, `:3761`, `:4362`,
`:4543`, `:4800`, `:5038`. Once channel 3 above stops writing into the same
slot as channel 1, the cleanest fix — matching what `eval.rs`'s driver
already does — is to delete the one-shot boolean gate entirely and check
`pending_exception.is_some() || pending_return.is_some()` **unconditionally**
at the top of every loop iteration (`eval.rs:8761` is the existing,
already-correct precedent: no gating flag, just an unconditional check,
because the local is guaranteed empty except when something legitimately
needs routing right now). Slice 4 should attempt this deletion; if some
generator-specific ordering turns out to depend on the one-shot gate, that
dependency itself is worth writing down as a comment, but the default
expectation is that it is no longer needed.

**Sync `TryExit`'s return re-entry needs an internal helper.** `TryExit`
(`generator_runtime.rs:1475-1499`) currently re-enters return routing by
synthesizing a fresh `IteratorState` and calling the *public*
`generator_return_state_machine`, whose own state-destructure elides the
pending fields with `..` (confirmed: calling `.return()` while a previous
exception/return is already parked silently drops it today — a related bug
this redesign must not reintroduce). Slice 1 should extract the core "given
a return value and the current `try_stack`, find the next handler and either
park-and-jump or complete" logic into a private helper analogous to
`route_generator_exception`, shared by the public `.return()` entry point
*and* `TryExit`'s internal re-injection, rather than having `TryExit` call
back into the public entry point.

## 4. TDD slices

All slices land as ordered commits in **one PR** (see §7 for why this cannot
be split across PRs). Each slice after slice 0 is red before its production
change and green after; slice 0 is a behavior-preserving mechanical
prerequisite guarded by the existing full regression suite instead of a new
test.

0. **Mechanical: introduce `PendingCompletion`, rename the field.**
   Add `PendingCompletion { Return(JsValue), Throw(JsValue), LoopControl(LoopControlTarget) }`
   to `types.rs`; rename `TryContextInfo::pending_loop_control` to
   `pending_completion: Option<PendingCompletion>` and update every existing
   read/write site (`types.rs`, `eval.rs`, `generator_runtime.rs`) to wrap/unwrap
   `PendingCompletion::LoopControl(..)` so behavior is byte-for-byte identical
   to today. No new test; gate is `cargo test --release` plus a full
   `uv run python scripts/run-test262.py` diffed against the `origin/main`
   baseline (must show zero regressions and zero new passes — this slice
   changes no behavior).

1. **Sync generator: nested `TryExit` no longer consumes an outer pending
   throw or return.**
   Red: `test262-extra/generator-nested-finally-preserves-outer-pending-throw.js`
   and `generator-nested-finally-preserves-outer-pending-return.js`, encoding
   the issue's first two confirmed-failure repros (nested `try`/`finally`
   inside an outer `finally`, driven by an internal `throw` and by an
   external `.return(42)` respectively), asserting the log order and final
   completion match Node. Green, per §3a: `route_generator_exception` parks
   the selected throw on `try_stack[depth].pending_completion` **only when
   the selected handler is a finally** (`finally_state`, not `catch_state`)
   — a catch-bound throw still flows through the existing transient local
   into `EnterCatch` (`:1513`), unchanged; extract a private return-routing
   helper (shared by the public `.return()` entry point and `TryExit`'s
   internal re-injection, per §3a's last point) that parks a selected return
   the same context-scoped way instead of `IteratorState.pending_return`;
   `TryExit` (`:1467-1507`) reads `finished.pending_completion` exclusively
   (drop the unscoped `pending_exception`/`pending_return` checks); `EnterFinally`
   (`:1530-1535`) stops needing any exception-saving logic (it already has
   none to remove — confirms the fix is additive here, not a removal). Also
   add the override-matrix and catch-preserves/uncaught-replaces cases here
   (`generator-pending-completion-override-matrix.js`): return→return,
   return→throw, throw→return, throw→throw, throw-caught-inside-preserves,
   throw-uncaught-replaces — one file, since they all exercise the same
   `TryExit` dispatch and are cheap to add together.

2. **Sync generator: `yield*` no longer drops a pending return.**
   Red: `test262-extra/generator-yield-star-preserves-outer-pending-return.js`,
   the issue's `try { yield 0; } finally { yield* [1, 2]; }` /
   `it.next(); it.return(42)` repro. Green: remove
   `IteratorState::StateMachineGenerator::{pending_exception, pending_return}`
   (forced by slice 0/1 already having moved their only remaining producer/consumer
   onto `try_stack`); the delegation-suspend struct literals at `:705-756` and
   `:1257-1303` stop hardcoding `pending_exception: None, pending_return: None`
   because the fields no longer exist — the pending completion rides along
   for free inside `try_stack`, which those same literals already snapshot
   unchanged.

3. **Plain async function: return/throw ownership through nested and
   suspending finalizers.**
   Red: `test262-extra/async-function-return-through-suspending-finally-with-loop-control.js`
   (issue's `try { return 42; } finally { while (true) { await 0; break; } }`,
   plus the `continue` variant) and
   `async-function-throw-through-nested-finally.js` (outer throw through a
   finally containing its own nested *normal* `try`/`finally` — this second
   case involves no suspension at all, so it is the direct test of §3a's
   claim that parking must happen at route time on the Rust struct, not via
   any `saved_finally_exception`-shaped local or field, suspended or not).
   Green: `route_return!` (`:8456-8517`) parks onto `try_stack[i].pending_completion`
   instead of the driver-local `pending_return`; the exception-routing block
   (`:8761-8901`) does the same for throw **only on the finally branch**
   (`is_catch == false`, per §3a) instead of `saved_finally_exception` — the
   `is_catch == true` branch keeps setting the transient `pending_exception`
   local exactly as today, since `EnterCatch` still consumes it the same
   way; `route_loop_control!` (`:8524-8591`) drops its blanket
   `pending_return = None; saved_finally_exception = None;` reset (context
   ownership makes it unnecessary — a loop-control completion now only
   replaces what the *same* context held, not whatever some unrelated
   context's driver-global slot happened to hold); `TryExit`/`EnterFinally`
   (`:9170-9246`) collapse to reading `finished.pending_completion` from the
   popped context only, the same shape as slice 1's sync-generator `TryExit`;
   remove `AsyncFunctionState::{pending_return, pending_loop_control, saved_finally_exception}`
   and simplify `async_fn_suspend_at_await`'s signature.

4. **Async generator: return/throw ownership, and `check_abrupt_on_resume`
   stops firing on a context-parked completion.**
   Red: async-generator counterparts of slice 1
   (`async-generator-nested-finally-preserves-outer-pending-throw.js`,
   `async-generator-nested-finally-preserves-outer-pending-return.js`) plus
   the issue's scenario 3,
   `async-generator-throw-delivered-after-finally-yields.js` (`try { throw 'E'; } finally { yield 1; yield 2; }`,
   asserting the rejection happens only after both yields, matching Node).
   Green, per §3a: same catch-consumes/finally-parks split and
   `TryContextInfo`-owned parking in `route_generator_exception`/the
   extracted return-routing helper, reused by the async driver;
   `TryExit`/`EnterFinally` (`:4534-4582`) collapse the same way as slice 1;
   the entry points that inject a fresh external `.throw()`/`.return()`
   (`async_generator_throw_state_machine_with_promise` and its `_return_`
   counterpart, `:5653-5832`) stop writing into the object's
   `pending_exception`/`pending_return` fields (deleted) and instead pass
   the fresh completion as an explicit parameter into
   `async_generator_next_state_machine_impl`; `check_abrupt_on_resume`
   (`:3414-3421` and its one-shot re-arm sites at `:3595`, `:3621`, `:3761`,
   `:4362`, `:4543`, `:4800`, `:5038`) is deleted in favor of an
   unconditional `pending_exception.is_some() || pending_return.is_some()`
   check at the top of every loop iteration, mirroring the pattern
   `eval.rs:8761` already uses correctly for plain async functions — the
   one-shot gate is what let a context-parked completion (channel 2, §3a)
   get misread as "check this once because it's fresh" (channel 3); with
   the two channels no longer sharing a slot, the gate serves no remaining
   purpose. If some ordering turns out to still depend on checking only
   once, that must be written down as a comment explaining why, not
   silently restored.

5. **Async generator: `yield*` no longer drops a pending return.**
   Red: `test262-extra/async-generator-yield-star-preserves-outer-pending-return.js`,
   the async analogue of slice 2. Green: remove
   `IteratorState::StateMachineAsyncGenerator::{pending_exception, pending_return}`;
   `yield_star_suspend_on_inner_result` (`:2699-2718`) stops zeroing
   `*pending_exception`/`*pending_return` before its `Await` (the fields are
   gone; the parked completion already rides in `try_stack`).

6. **GC: a return/throw payload parked only in a suspended try context stays
   rooted.**
   Red: `test262-extra/generator-pending-completion-gc-rooting.js` (sync,
   `features: [generators, host-gc-required]`, no `flags: [async]` needed —
   force `$262.gc()` between a `.next()` that suspends inside a finally
   holding a parked return object and the resume that observes it) and
   `test262-extra/async-function-pending-completion-gc-rooting.js`
   (`flags: [async]`, `features: [host-gc-required]`, following the pattern
   in `async-generator-await-return-pending-gc-rooting.js`, forcing GC while
   an `AsyncFunctionState` sits scheduler-held with a parked return object
   reachable only through `try_stack`). Green: the `gc.rs` tracing added in
   §3 — this slice should require *no* production change beyond what slices
   1–5 already did if the tracing was added alongside them; if written last,
   this is the slice that actually adds it, and its red state is a crash or
   a wrong value after collection, not a panic (mark-and-sweep would simply
   collect the payload and leave a dangling/`undefined` value at resume).

7. **Docs.** Update `CONTEXT.md`'s **Loop Control** entry and add the new
   ADR (§3). No test; `./scripts/lint.sh` covers prose/markdown gates that
   apply to docs.

## 5. Test surface

Targeted `test262/test/...` directories to run after each slice (in addition
to `test262-extra/` and `run-custom-tests.py`):
- `test262/test/language/statements/try/`
- `test262/test/language/statements/generators/`
- `test262/test/language/expressions/generators/` and `.../yield/`
- `test262/test/language/statements/async-generator/` (and
  `language/expressions/async-generator/` if generator-expression forms
  exist there)
- `test262/test/built-ins/GeneratorPrototype/`
- `test262/test/built-ins/AsyncGeneratorPrototype/`
- `test262/test/language/statements/async-function/`
- `test262/test/language/expressions/async-function/`
- `test262/test/language/statements/for-of/`, `.../for-await-of/`,
  `.../for-in/` (loop-control interaction, already covered by #717's tests —
  regression-only here)
- `test262/test/language/statements/break/`, `.../continue/`,
  `.../labeled/` (loop-control regression)

New `test262-extra/` files (test262 frontmatter, one `esid` per file, named
per §4): the eight files listed in slices 1, 2, 3 (×2), 4 (×3), 5, 6 (×2).
Every one of them encodes a scenario the issue confirmed diverges from Node
today, which test262 itself does not (and by design cannot) cover, since it
is written against observable behavior of *some* conforming engine and has
no notion of "this specific internal driver's shared-slot bug" — the closest
existing test262 coverage (ordinary `try`/`finally`/generator tests) already
passes today, precisely because those tests don't happen to nest a second
`try`/`finally` inside the first one's `finally`, or don't combine that
nesting with suspension/delegation.

Final gate before considering the PR ready: full
`uv run python scripts/run-test262.py` (all of `language/`, `built-ins/`,
`annexB/`, `intl402/`), diffed against the `origin/main` baseline
(`test262-pass.txt`, read via `--baseline-ref` default) — zero regressions,
and the new passes should correspond exactly to tests the fix newly makes
pass, if any test262 tests (not just test262-extra) happen to exercise these
scenarios already. `cargo test --release` for the Rust unit-test suite
(`types.rs`'s `completed_state_machine_generator_tests` module needs updating
for the removed fields, per §3). `./scripts/lint.sh` for formatting/clippy.

## 6. Regression risk

This change sits directly in the shared dispatch loop every generator, async
generator, and async function body runs through (`TryEnter`/`TryExit`/`EnterFinally`/`EnterCatch`
match arms in both `eval.rs::async_function_resume` and
`generator_runtime.rs`'s two driver-impl functions) — the same functions PR
#717 touched for loop control, which is exactly why that PR's full-suite run
(99,911/99,911) is the right bar here too. Specific risk areas:
- **Try/catch/finally baseline**: every test262 file under
  `language/statements/try/` plus every generator/async-generator/async-function
  test that happens to use `try`/`catch`/`finally` internally (a large
  fraction of the generator and async-function suites) exercises the
  rewritten `TryExit` dispatch. A mistake here would show up as a broad,
  not a narrow, regression.
- **`for-of`/`for-await-of` iterator closing**: `route_return!`/`route_loop_control!`/`route_generator_exception`/`route_generator_loop_control`
  all interleave for-of unwinding with the handler search; reordering when a
  completion gets parked-on-context vs. routed could shift *which* iterators
  close before vs. after a given finally, which is exactly the class of bug
  #717 fixed for loop control. Regression guard: the existing
  `generator-loop-control-closes-for-of-iterators.js`,
  `async-generator-loop-control-closes-for-await-iterators.js`, and the
  `for-of`/`for-await-of` test262 suites.
- **`await using` / disposal ordering**: `route_return!`/`route_loop_control!`
  in `eval.rs` interleave with `unwind_scopes_to!`/`DisposeThen::ScopeCross*`;
  those call sites are not being redesigned, only the storage their result
  feeds into, but they are high-surface-area and must be re-verified. Guard:
  the `*-await-using-*` test262-extra suite (dozens of files already listed
  in `ls test262-extra/`).
- **GC rooting**: this is the one place behavior *must* change (see §3/§4
  slice 6) — `try_stack` becomes traced for the first time. Missing a trace
  site is a use-after-free-shaped bug (a collected `JsValue` surfacing as
  garbage/undefined on resume), not a compile error, so the forced-GC test262-extra
  tests are load-bearing, not decorative.
- **`stays_inside_running_finally`** (`generator_runtime.rs:12-17`) and its
  "jump that leaves replaces the entering completion" comment describe
  exactly the invariant now being generalized to throw/return; if routing
  ends up subsuming this helper, the existing
  `generator-loop-control-through-suspending-finalizer.js` /
  `async-generator-loop-control-through-suspending-finalizer.js` files are
  the regression guard that the generalization didn't change loop-control's
  own (already-correct) behavior.
- **Not touched, low risk**: `property.rs` (no property MOP involvement),
  the exhaustive `ObjectKind` match in `gc.rs` (no new `ObjectKind` variant —
  `TryContextInfo` lives inside existing variants' payloads, not as a new
  variant), the bytecode fast path (`src/bytecode/` has no reference to
  `TryContextInfo`/`pending_loop_control`/`LoopControlTarget` today, and
  `bytecode_enabled` defaults to `false`), and the Node-compat library
  harnesses (none of `decimal.js`/`acorn`/`zod`/etc.'s green corpora are
  documented as generator/async-generator-heavy in a way that would newly
  exercise nested finalizers beyond what test262 already covers — rerun
  `./scripts/run-library-tests.sh acorn` as a cheap spot-check since it's
  the fastest green one, not as a required gate).

## 7. Out of scope

- **Unifying the three drivers.** The issue explicitly rules this out: "Do
  not unify the three full drivers; their queue, disposal, iterator-closing,
  and promise mechanics legitimately differ. Share only the completion-routing
  invariant and small stack helpers." This plan shares only the
  `PendingCompletion` type and the routing *shape*, not a merged driver.
- **Splitting this into multiple PRs.** `TryContextInfo` is a single `struct`
  shared by `AsyncFunctionState` and both generator `IteratorState` variants;
  changing its `pending_loop_control` field to `pending_completion` is a
  breaking Rust-type change that every call site in all three drivers must
  update in the same commit range to compile at all, and the issue's
  acceptance criteria explicitly require sync-generator, async-generator,
  *and* plain-async-function coverage before the issue can be called closed.
  There is no smaller slice that both compiles and closes the issue; §4's
  slices are ordered commits within one PR instead.
- **Removing `stays_inside_running_finally` outright.** Keep it (or fold its
  logic into the generalized routing) only if the implementation naturally
  subsumes it; do not go hunting for an unrelated simplification here beyond
  what the routing change forces.
- **Bytecode fast path.** Not touched — see §6. If `bytecode_enabled` is ever
  turned on for generators/async functions in the future, this same defect
  class would need its own investigation there; out of scope now.
- **Rolling the `test262-pass.txt` baseline forward.** Per constraints, that
  is a `main`-branch operation (`--update-baseline`) and is not part of this
  plan regardless of how many new tests pass.
- **Staging test262 coverage** (`test262/test/staging/`). Not part of the
  default runner per `CLAUDE.md`; not required by the issue's acceptance
  criteria.
- **Formatting/unrelated cleanup** in files this change must touch anyway
  (e.g. no drive-by renames of unrelated identifiers in `generator_runtime.rs`
  beyond what the field rename mechanically forces).
