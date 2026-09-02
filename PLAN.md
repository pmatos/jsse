# Plan: issue #540 — resolve generator/async BODY rows to function names

## 1. Problem restated

The `perf-counters` `BODY` ranking (#537) attributes every generator, async
function, and async generator body to one flat synthetic bucket,
`<generator/async body>`, instead of the originating function's own name —
so a workload whose hot path is a generator reports "most of the work is in
some generator" without saying which one. `exec_body` (the entry point these
bodies replay/step through) carries no function identity because generator
and async state machines are executed via `IteratorState::StateMachineGenerator`
/ `StateMachineAsyncGenerator` / `AsyncFunctionState`, none of which store the
originating function object's id. This plan resolves that: each of the three
live state-machine creation sites captures the calling function's already-
computed `(name, id)` `BodyKey` and carries it (via the state machine, which
every resume/carry-forward already `Rc`-clones opaquely) to the per-step
executor, so the step's tree-walker work lands in a `BODY` row named after
the function instead of the flat bucket.

## 2. Spec basis

N/A: no JavaScript behavior change. This is opt-in diagnostic instrumentation
(`--features perf-counters`), absent from the shipped binary, that changes
only what a counter *report* prints to stderr. No parsing, evaluation, or
observable-from-JS behavior is touched — every default-build code path
through the three call sites this plan changes is byte-identical before and
after (same `exec_body_inner` call, same arguments), and the non-default
build only changes *labels* in a diagnostic table (§6 explains the invariant
that guarantees the underlying counts don't move either).

## 3. Files to touch

Engine (all `src/interpreter/`):
- `generator_transform.rs` — add a `perf-counters`-gated field to
  `GeneratorStateMachine` holding the resolved `BodyKey` (or `None` for the
  module top-level-await case, which has no function).
- `eval.rs` — at the three live state-machine creation sites (sync generator,
  async generator, plain async function via `call_async_function`), resolve
  `self.perf_body_name(o.id)` once, while the function object is still
  reachable, and stash it on the freshly built state machine. Add the new
  `exec_state_machine_body` method (beside `exec_body`'s existing helpers is
  fine, but see `exec.rs` below — either file works; put it in `exec.rs` next
  to `exec_body_attributed`, since that's where the low-level
  `enter_ast_body`/`leave_ast_body`/`body_non_function` plumbing already
  lives). Swap the `async_function_resume` step-executor's `exec_body` call
  (currently line 8787) to call it.
- `exec.rs` — add `exec_state_machine_body`; add the `is_function_invocation:
  bool` parameter to the two existing `enter_ast_body` call sites in this
  file (both stay `false`, i.e. unchanged behavior, just explicit now).
- `eval/generator_runtime.rs` — swap the two live step-executor `exec_body`
  calls (`generator_next_state_machine_impl`, currently line 770; and
  `async_generator_next_state_machine_impl`, currently line ~4627) to call
  `exec_state_machine_body`. **Do not** touch the two dead "replay" paths
  (`generator_next`/`generator_return`/`generator_throw` reading
  `IteratorState::Generator`, and `async_generator_next` reading
  `IteratorState::AsyncGenerator`) — see §6, they have no live construction
  site and are out of scope.
- `perf_counters.rs` — add the `is_function_invocation: bool` parameter to
  `enter_ast_body`, store it in the `ast_body_stack` tuple, and use it in
  `leave_ast_body` in place of the current `key.1 != SYNTHETIC_BODY_ID`
  inference (that inference stops being valid once generator/async rows also
  carry non-synthetic ids). Update the doc comments on `enter_ast_body`,
  `leave_ast_body`, `SYNTHETIC_BODY_ID`, and `name_non_function_body` to
  describe the new split. Update the ~9 in-file unit-test call sites to pass
  the flag that reproduces today's behavior (`true` wherever the id passed
  is not `SYNTHETIC_BODY_ID`, `false` where it is).
- `mod.rs` — update the one remaining `enter_ast_body` call site (module
  body, `execute_module_body_sync`) to pass `is_function_invocation: false`
  (unchanged behavior, now explicit). No other change: `execute_async_module`
  (the top-level-await module path, `AsyncFunctionState` with no function
  object) needs no edit — the new `GeneratorStateMachine` field defaults to
  `None` there, which is exactly today's `<generator/async body>` behavior.

Docs:
- `CLAUDE.md` (repo root; `AGENTS.md` is a symlink to it) — update the
  "Execution Counters" section: replace "resolving the generator/async
  bucket to individual function names is jsse#540" with a description of the
  resolved behavior, and add the caveat the issue asks for explicitly: a
  resolved generator/async `BODY` row's invocation column counts
  state-machine steps (replay/resume steps), not calls — same semantics
  `body_non_function_execs` already documents, now visible per-function
  instead of only in aggregate. Note that `<eval>`, `<script body>`, and
  `<module body>` remain synthetic (out of scope — §7).
- Do **not** edit `docs/perf/2026-08-26/mandreel-bytecode-work-share.md` — it
  is a dated measurement snapshot of one specific data collection, not living
  documentation; mandreel's own report already notes "no BODY row acquired a
  `#id` suffix" for that run, so this change doesn't invalidate anything it
  states.

Tests:
- `tests/perf_counters_report_paths.rs` — update
  `generator_and_eval_work_is_not_credited_to_the_caller` in place, and add
  one new test for the async-function path. See §4.

## 4. TDD slices

Write the tests first so the whole sequence is red until the last production
slice; slices 1-3 are invisible plumbing that must not change any existing
test's output, so run the full existing suite after each to confirm nothing
moved before the intended slice.

1. **Red**: rewrite
   `tests/perf_counters_report_paths.rs::generator_and_eval_work_is_not_credited_to_the_caller`.
   Keep the existing program (`gen`, `caller`, `evalCaller`) and keep the
   `eval_body`/`caller` assertions unchanged. Replace the
   `row("<generator/async body>")` lookup with `row("gen")` (the resolved
   name) and keep the `> 1000` bound. Add a new assertion reading
   `PERF\tast_units_in_functions` from stderr and asserting it stays small
   (well under the generator's exclusive total — bound it against `caller`'s
   own row, e.g. `< 100`, matching the existing `caller` bound) — this
   encodes "relabelling, not a recount" directly: if slice 5 accidentally
   passes `is_function_invocation: true` for the resolved generator row, this
   assertion catches the corruption the `ast_units_in_functions` doc comment
   warns about. This test fails to compile-then-pass now (no code changed
   yet) — confirm it fails for the *expected* reason (`row("gen")` is `None`)
   before proceeding.
2. **Red**: add a new test, `named_async_function_gets_its_own_body_row`,
   structurally parallel to the generator test but exercising
   `call_async_function`/`async_function_resume` instead: a named async
   function with a loop-heavy body awaited from a caller, asserting the
   async function's own name appears as a `BODY` row with a large exclusive
   count and the caller's row stays small. This is the second code path
   (`AsyncFunctionState`) the generator test doesn't cover, and it's the one
   most likely to regress independently since it's threaded through
   `call_async_function`'s new parameter rather than a state-machine
   creation site inline in the `is_generator`/`is_async` match arms.
3. **Green (plumbing, no behavior change)**: `perf_counters.rs` — add
   `is_function_invocation: bool` to `enter_ast_body`'s signature and the
   `ast_body_stack` tuple; update `leave_ast_body` to branch on it instead of
   `key.1 != SYNTHETIC_BODY_ID`; update all existing call sites (production
   and the in-file unit tests) to pass the value that reproduces current
   behavior. Run `cargo test --release --features perf-counters` — every
   existing `perf_counters.rs` unit test and every `tests/
   perf_counters_report_paths.rs` test except the two just written must
   still pass unchanged (the two new/changed ones still fail, for the
   expected reason: no name resolution exists yet).
4. **Green (plumbing, no behavior change)**: `generator_transform.rs` — add
   `#[cfg(feature = "perf-counters")] pub(crate) perf_key: Option<crate::
   interpreter::perf_counters::BodyKey>` to `GeneratorStateMachine`; default
   `None` at both literal construction sites (`transform_generator_inner_opts`
   and `create_simple_machine`). Nothing reads the field yet. Confirm
   `cargo build --release --features perf-counters` and plain `cargo build
   --release` both still compile.
5. **Green (the fix)**: `eval.rs` / `exec.rs` / `generator_runtime.rs`:
   - At the sync-generator creation site (`is_generator` branch, currently
     around line 5883-5908) and the async-generator creation site (`is_async
     && is_generator` branch, currently around line 5698-5723): after
     building the state machine and before wrapping it in `Rc::new`, under
     `#[cfg(feature = "perf-counters")]`, set `.perf_key = Some(self.
     perf_body_name(o.id))` (the calling function object is provably alive
     here — it's mid-`[[Call]]` — so this cannot hit the GC hole described
     below).
   - Add a `_func_obj_id: u64` parameter to `call_async_function` (name
     follows the `Err(_e)` precedent at `exec.rs:95` — read only under
     `--features perf-counters`), threaded from its single call site (~line
     5563, where `o.id` is already in scope). Inside, after `let sm = Rc::
     new(transform_async_function(...))` (currently ~line 8277-8282), same
     pattern: resolve and stash `perf_key` before the `Rc::new` (i.e. build
     the `GeneratorStateMachine` value first, set the field, then wrap it).
   - Add `exec_state_machine_body(&mut self, body: &Body, env: &EnvRef,
     _state_machine: &GeneratorStateMachine) -> Completion` to `exec.rs`,
     mirroring `exec_body_attributed`: increments `body_non_function`,
     resolves `(name, id)` from `_state_machine.perf_key.clone()` — falling
     back to `(self.perf.name_non_function_body.clone(), SYNTHETIC_BODY_ID)`
     when `None` (the module top-level-await case, unchanged from today) —
     calls `enter_ast_body(name, id, false)` (the `false` is the point of
     this whole plan: generator/async work stays *excluded* from
     `ast_units_in_functions`, matching the existing documented invariant
     that its denominator, `body_ast`, counts real `dispatch_body`
     invocations only, not replay steps), then `exec_body_inner`, then
     `leave_ast_body`.
   - Swap the three live call sites (`generator_next_state_machine_impl`
     line 770, `async_generator_next_state_machine_impl` line ~4627,
     `async_function_resume` line 8787) from `self.exec_body(&state_machine.
     states[current_id].body, &term_env)` to `self.exec_state_machine_body(
     &state_machine.states[current_id].body, &term_env, &state_machine)`.
   - Run the two new/updated tests — both green now. Run the full
     `tests/perf_counters_report_paths.rs` and `perf_counters.rs` unit-test
     suites — everything else still green (confirms the relabelling is
     scoped to exactly the intended rows).
6. **Docs**: update `CLAUDE.md`'s Execution Counters section per §3. No code
   change; verify `./scripts/lint.sh` (or whatever the doc/typo gate is)
   stays clean.

## 5. Test surface

This is a Cargo-feature-gated diagnostics path with no JS-visible behavior,
so there is no test262 directory that exercises the *change* directly.
However, since the change touches the generator/async-function/async-
generator execution path (even though every default-build branch through it
is unchanged — `exec_state_machine_body`'s `#[cfg(not(feature =
"perf-counters"))]` arm is a straight call to `exec_body_inner`, byte-
identical to today's `exec_body`), run these targeted test262 directories as
a sanity check that the call-site edits didn't perturb control flow:
`test262/test/language/statements/generators/`,
`test262/test/language/expressions/generators/`,
`test262/test/language/statements/async-function/`,
`test262/test/language/expressions/async-function/`,
`test262/test/language/statements/async-generator/`,
`test262/test/language/expressions/async-generator/`,
`test262/test/built-ins/GeneratorFunction/`,
`test262/test/built-ins/AsyncFunction/`,
`test262/test/built-ins/AsyncGeneratorFunction/`. Then run the full suite
(`uv run python scripts/run-test262.py`) before opening the PR — it must not
move `test262-pass.txt` relative to `origin/main` (do not pass
`--update-baseline`).

The actual behavior gates for this issue:
- `cargo test --release --features perf-counters` — runs both the
  `perf_counters.rs` in-file unit tests and `tests/
  perf_counters_report_paths.rs` (the latter is `#![cfg(feature =
  "perf-counters")]`-gated and spawns the built binary as a child process,
  so it requires the feature-enabled binary to exist — `cargo build
  --release --features perf-counters` first, or let `cargo test` build it).
- `cargo test --release` (default, no feature) — confirms the non-perf-
  counters build compiles and every other test is unaffected; per this
  workspace's bin-only-crate note, `cargo test --release --bin jsse` is the
  narrower form if the full suite is slow.
- `cargo build --release` and `cargo build --release --features
  perf-counters` — both must compile cleanly (the `perf_key`/
  `_func_obj_id`/`_state_machine` naming choices exist specifically to keep
  both configurations warning-clean under `clippy -D warnings`).
- `./scripts/lint.sh`.

## 6. Regression risk

- **The `ast_units_in_functions` invariant is the main correctness risk.**
  Before this change, whether a body's exclusive AST units counted toward
  `ast_units_in_functions` was inferred from `key.1 != SYNTHETIC_BODY_ID`.
  After this change, generator/async `BODY` rows carry a real (non-
  synthetic) id but must still be *excluded* from that counter, because its
  published denominator (`body_ast`) counts `dispatch_body` invocations
  only, and generator/async replay steps are not invocations in that sense
  (a single generator call can register ~4 steps per yield). This is why the
  plan introduces an explicit `is_function_invocation` flag instead of
  continuing to infer it from the id — the id and the invocation-accounting
  question are now independent, and conflating them again would silently
  reintroduce the exact "2,966.82 -> would move" style corruption the #537
  review already fixed once. The new `ast_units_in_functions` assertion in
  slice 1 exists specifically to catch a regression here.
- **GC lifetime of the resolved name.** A generator object's traced fields
  (`state_machine`, `func_env`, ...) do not reference the originating
  function object — only its `.prototype_id` (pointing at the realm's
  generator prototype, not the user function) keeps anything alive on that
  side. Resolving the name *lazily* at first-step time (e.g. inside
  `exec_state_machine_body` from a raw `func_obj_id`) would risk the
  function object having been collected between generator creation and
  first `.next()` (e.g. an IIFE generator expression with no surviving
  binding to the function itself: `(function* g(){...})()`). This plan
  avoids the hole entirely by resolving `perf_body_name` once at creation
  time, while the function is provably reachable (mid-`[[Call]]`), and
  carrying the already-resolved `(Rc<str>, u64)` pair through — not the raw
  id. No new GC rooting is needed because nothing new is stored that GC
  doesn't already discard freely (an `Rc<str>` name and a `u64` are not
  traced objects).
- **`enter_ast_body`/`leave_ast_body` signature change** touches every
  caller (4 production sites, ~9 unit tests) — a mechanical but wide diff;
  the risk is a missed call site defaulting to the wrong flag and silently
  shifting `ast_units_in_functions` for an unrelated body kind. Mitigated by
  running the full existing test suite after slice 3 (before any new
  behavior exists) to confirm zero output drift.
- **Shared machinery leaned on**: the tree-walker `exec_statement`/
  `eval_expr` counters (`ast_stmts`/`ast_exprs`, unchanged — this plan only
  changes attribution, not counting), `perf_name_cache`/`perf_next_body_seq`
  (reused as-is, not modified), and the `Rc`-clone-carries-fields-opaquely
  property of `IteratorState::StateMachineGenerator`/
  `StateMachineAsyncGenerator`/`AsyncFunctionState`'s `state_machine:
  Rc<GeneratorStateMachine>` field, which is what lets this plan avoid
  touching the ~170+ carry-forward struct-literal sites in
  `generator_runtime.rs` (see §7). Not touched: the bytecode VM/compiler
  (generator/async bodies never reach `dispatch_body`, so `bytecode_enabled`
  is irrelevant here), the property MOP, and GC rooting/`gc_safepoint()`
  (no new traced state is introduced).
- **Pre-existing gap, not changed by this plan**: terminator expressions
  (a `yield` value, a `return` value, a `ConditionalGoto` condition) are
  evaluated by the state-machine driver *outside* the
  `enter_ast_body`/`leave_ast_body` frame for the current state's body, so
  their `eval_expr` units still land on whatever frame is active at the call
  site (today: the caller; after this change: still the caller, since this
  plan doesn't touch where the driver evaluates terminators). This means a
  resolved generator's `BODY` row is not perfectly exhaustive of "all work
  attributable to this generator" — it's the state *bodies'* work, matching
  exactly what the flat `<generator/async body>` bucket already reported
  before this change. Flag this for the implementer so exact-number
  expectations in manual testing don't chase a discrepancy that isn't a bug
  in this PR.

## 7. Out of scope

- **`<eval>`, `<script body>`, `<module body>` stay synthetic.** The issue
  title says "generator/async and eval," but AGENTS.md's own line ("resolving
  the generator/async bucket to individual function names is jsse#540") and
  the Validation section (which only asks for "a named generator appears as
  its own BODY row") both scope this narrowly. `eval`'d code, top-level
  scripts, and modules are not named functions — there is no `JsFunction::
  User.name` to resolve them to — so extending resolution to them is a
  different, larger design question (e.g. "attribute eval to its calling
  function's name" is a plausible follow-up, but a different mechanism: it
  would key off the *call site*, not a function identity). **PR title**:
  `perf(counters): resolve generator/async BODY rows to function names` (no
  "and eval" — the squash subject is taken verbatim from this title). State
  the eval/script/module scoping decision explicitly in the PR body so a
  reviewer doesn't read it as an oversight.
- **The dead "replay" generator paths are not touched.**
  `IteratorState::Generator` and `IteratorState::AsyncGenerator` (read by
  `generator_next`/`generator_return`/`generator_throw`/`async_generator_next`
  in `generator_runtime.rs`) have no live construction site anywhere in the
  codebase — every real generator/async-generator call constructs
  `StateMachineGenerator`/`StateMachineAsyncGenerator` instead (confirmed by
  grepping for `IteratorState::Generator {` / `IteratorState::AsyncGenerator
  {` as constructors: none exist outside `gc.rs`'s trace match and
  `generator_runtime.rs`'s own read/reassign sites). Removing this dead code
  is a legitimate but unrelated cleanup — worth its own follow-up issue, not
  bundled into this relabelling PR.
- **Threading `func_obj_id` through every `IteratorState::StateMachineGenerator`
  / `StateMachineAsyncGenerator` struct-literal site (the issue's own
  "option 1, smallest change") is rejected**, not deferred: grepping the
  codebase shows 67 `StateMachineGenerator {` sites and 107
  `StateMachineAsyncGenerator {` sites in `generator_runtime.rs` alone (carry-
  forward re-saves at every suspension point across `yield*` delegation,
  try/catch, and loop-control edge cases), almost all pattern-matched with a
  trailing `..` that would silently *not* propagate a naively-added field.
  The chosen design (§3-5) touches 3 creation sites and 3 step-executor call
  sites instead, because `state_machine: Rc<GeneratorStateMachine>` is
  already carried through every one of those 174 sites opaquely via `Rc`
  clone/move — this is `Body::key()`'s existing "side table keyed by stable
  identity" pattern (option 3 in the issue), specialized to reuse the
  `Rc<GeneratorStateMachine>` that's already the right shape instead of
  inventing a new side table.
- **No `--update-baseline` run.** Not expected to be needed (no JS behavior
  changes), but if the full test262 run surfaces any drift, that is a signal
  something in this plan's "byte-identical default build" claim is wrong —
  investigate before assuming the baseline needs rolling forward, and in any
  case that operation belongs to `main`, not this branch.
