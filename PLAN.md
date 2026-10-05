# Plan: issue #845 — await-using for-loop heads and switch-with-lexical-sibling still drain disposal inline

## 0. Findings that reshape the issue's own scope

Before planning the fix, each of the issue's three named items was probed against `node`
(current binary built from this branch, `./target/release/jsse`) and against the unit tests
that already pin `generator_analysis.rs::scan_await_using`'s classification. Two of the three
items turn out to already be fixed on `main`; the third is real but narrower than the issue's
prose suggests, and surfaces one additional, bigger, out-of-scope bug along the way. A
`gh issue comment 845` summarizing this (so the next reader doesn't re-litigate it) is part of
this plan's exit, written by the implementation stage once the fix lands.

- **Item 1a, C-style `for (await using x = init; ...)` head** — already fixed, by `#787`
  (`ForStatement::disposes_at_head`, `generator_analysis.rs:1408-1410`,
  `await_using_for_head_scope`). `test262-extra/await-using-for-head-dispose-tick-alignment.js`
  passes today (ran it: 100%, all scenarios).
- **Item 1b, `for (await using x of y)` head** — already correct in practice, confirmed by
  running `test262-extra/await-using-for-of-head-dispose-tick-alignment.js` and
  `async-generator-await-using-for-of-head-dispose-tick-alignment.js` (both 100% today) *and* by
  three hand-written node-vs-jsse probes covering shapes those files don't: a lexical sibling
  next to the loop, a nested `await using` block inside the loop body, and both together. All
  three match `node` tick-for-tick. `scan_await_using`'s `ForOf` arm still classifies this shape
  `Blocked` (the guard the issue cites), but that classification is not actually consulted for
  tick alignment — `stmt_contains_await_using_head` (`generator_transform.rs:685-687`,
  independent of `scan_await_using`) already forces full state-machine lowering whenever a
  `ForOfStatement::disposes_at_head()` is true, and the `ForOfHead` terminator
  (`eval/generator_runtime.rs:5165` onward) already drives the per-iteration disposal through
  `DisposeCursor`/`GeneratorDisposal`, suspending correctly. No code change needed; a
  regression test pins the current (correct) behavior so this doesn't silently rot.
- **Item 2, switch with a lexical sibling** — already fixed, by `#853` (merged today,
  2026-10-05, closing `#841`). `scan_switch_body` no longer exists;
  `generator_analysis.rs::scan_flattened_list` now handles `switch` identically to
  `Block`/`try`. Ran `test262-extra/await-using-switch-case-block-dispose-tick-alignment.js`
  and `async-function-switch-case-block-scope.js`: 100% today.
- **Item 3, the audit** — delegated to a read-only agent that walked every named call site
  (`exec.rs`'s loop/switch `dispose_resources` sites, `close_for_of_loop`,
  `unwind_async_for_of_loops`, all `dispose_resources`/`run_dispose_cursor_blocking` sites in
  `eval/generator_runtime.rs`). Full breakdown in §5/§6 below. Headline result: the only
  *confirmed, reachable, fixable-now* gap is a **`using` (sync, non-`await`) `for`/`for-of` head
  wrapping a nested `await using` block**, which is the literal "same `blocked_unless_none`
  guard" the issue points at — just for the `Using` arm of that guard, not the `AwaitUsing` arm
  the issue's prose focuses on. Confirmed with node-vs-jsse probes (reproduced below under
  Spec basis).

### The confirmed, in-scope bug

```js
async function f() {
  for (using r of [{ [Symbol.dispose]() { /* sync */ } }]) {
    { await using a = { [Symbol.asyncDispose]() { /* async */ } }; }
  }
}
```

and the C-style equivalent `for (using r = …; ; ) { { await using a = …; } }`. When this is the
*only* construct in an async function/async generator needing suspension-awareness (no other
literal `await`/`yield` forces lowering), `generator_transform.rs::transform_generator_inner_opts`
picks `create_simple_machine` — the whole body runs on the plain tree-walker, so the nested
`await using` block's disposal drains the job queue inline instead of suspending at its `Await`.
Probe (`$TMPDIR`-scoped scratch, not `/tmp`, for the implementation stage to reuse):

```
node:  ["disp-inner","sync-end","w1","body","disp-sync","after","w2","settled","w3","w4"]
jsse:  ["disp-inner","w1","body","disp-sync","after","sync-end","w2","settled","w3","w4"]
```

`sync-end` (logged synchronously by the caller right after invoking the async IIFE) comes before
`w1` on node but after it on jsse — the classic "blocking disposal drained a microtask before
returning control to the synchronous caller" signature from `#665`'s own investigation.

Root cause, traced precisely: `has_suspendable_await_using_block` (the predicate
`transform_generator_inner_opts`'s `create_simple_machine` gate uses,
`generator_transform.rs:621`) is `scan_await_using(stmt) == Isolatable`. For a `using`-headed
`for`/`for-of`, `scan_await_using`'s `For`/`ForOf` arms unconditionally return `Blocked` via
`body.blocked_unless_none()` regardless of whether the body itself is `Isolatable` — so the gate
never sees the nested block and picks the no-lowering fast path. Confirmed this is *purely* a
top-level "should we lower at all" gate bug, not a disposal-mechanism bug: when lowering is
forced by an unrelated `await 0` elsewhere in the same function, the identical
`using`-head-wraps-`await-using`-block shape matches node exactly (probed both the for-of and
C-style-for forms this way). This also matches why item 1b needed no fix — its guard is
`Blocked` too, but a *different*, unconditional gate (`stmt_contains_await_using_head`) already
forces lowering for the `AwaitUsing` arm, independent of `scan_await_using`.

### A bigger bug found along the way — explicitly out of scope

Probing the C-style `for (using r = …; ; )` head further (to check whether the `Using` arm's fix
should mirror `await_using_for_head_scope`, the mechanism `#787` built for `AwaitUsing`) surfaced
a second, deeper bug: once a `using`-headed C-style `for` loop *is* lowered (for any reason —
reproduces identically in a plain sync generator, so this isn't async-specific) and exited via
`break`, disposal fires **after** the code following the loop has already run, instead of at the
loop's own exit:

```
node (sync generator): "disposed" (x2 — a separate node-only double-dispose quirk, see below), then "after"
jsse (sync generator):  "after", then "disposed"
```

This is `transform_for_statement`'s generic `CopyForward`-based lowering not understanding
`using`/`await using` heads at all (per its own doc comment and the issue's own text: "a
distinct, larger change from #665's per-entry-scope relaxation"), not a tick-alignment-only gap.
Fixing it needs `transform_for_statement` (or a `Using`-extended `await_using_for_head_scope`,
gated by a *new* predicate — **not** by widening `ForStatement::disposes_at_head()`, which
`stmt_has_suspension`/`stmt_contains_await_using_head` key on for its narrower "head disposal may
`Await`" meaning) to give a `using` C-style head its own scope and correct loop-control-exit
disposal timing, mirroring `#787`'s rewrite but without the `Await` aspect. That is a materially
larger, separate change with its own regression surface (loop-control routing, not just the
`create_simple_machine` gate). **Not attempted in this PR.** The implementation stage files a new
`gh issue` for it (title suggestion: "bug: `using`-headed C-style `for` disposes after loop-exit
code on `break`"), referencing this investigation, and notes the node double-dispose quirk below
so nobody copies it into a test's expected array.

Aside, not actionable: node discloses the C-style `using` resource **twice** on some exits (e.g.
`for (using r = D; false; ) {}` without even entering the body disposes twice on node, once on
jsse). `using`'s `ForBodyEvaluation` treats the head as const-like with no per-iteration copy
(`CreatePerIterationEnvironment` has nothing to copy for a `using`/`await using` binding), so a
single disposal at loop-exit is the spec-correct count; jsse's single disposal looks right and
node's double-dispose looks like a V8-specific quirk. Do not use node's count as the oracle for
any new test — derive the expected disposal count from the spec text quoted in test262's own
`for-of`/`for`/`using` `esid` files instead (see §1).

## 1. Spec basis

`N/A` does not apply — this changes observable disposal-timing behavior — but the local `spec/`
submodule cannot be cited by clause number here: it is pinned at `270a490b` (2026-01-21), and
`grep -ic dispos spec/spec.html` returns `0` — **Explicit Resource Management is not present in
this checkout of `spec/spec.html` at all** (the pin predates, or otherwise lacks, that merge).
This is a pre-existing gap in the pinned submodule, not something this PR's scope covers (bumping
the pin is a `spec/`-submodule content change this repo treats as a separate, deliberate
operation, and `spec/` must never be modified directly).

Authoritative clause text instead comes from `test262`'s own `esid`-tagged metadata, the same
precedent every prior ERM PR in this repo (`#665`, `#787`, `#841`/`#853`) already followed:

- **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`**
  (`ForIn/OfBodyEvaluation`) — step effectively numbered 9.k in the existing
  `test262-extra/await-using-for-of-head-dispose-tick-alignment.js`'s `info:` block: each
  iteration's `DisposeResources(iterationEnv.[[DisposeCapability]], result)` call, which `Await`s
  when any disposed resource's hint is `async-dispose`. This governs the for-of fix: a `using`
  (sync-dispose hint) head's own disposal never needs this `Await`, but a *nested* `await using`
  block reached through the loop body is disposed through its own, separate
  `DisposeResources`/`Await`, which must suspend the function rather than draining the queue.
- **`DisposeResources ( disposeCapability, completion )`** (quoted inline in the same existing
  test file): steps 3.f.i-ii (`needsAwait` set per-resource from its `DisposeMethod`'s hint) and
  step 4 (`Perform ! Await(undefined)` once, if any disposed resource needed it and none has
  already awaited). This is the operation whose `Await` must land on a real suspension point, not
  a blocking drain.

No *new* syntax or semantics are introduced — the fix only corrects which existing constructs the
engine recognizes as needing the state-machine lowering that already implements
`DisposeResources`'s `Await` correctly (per `ADR-2026-09-21-2015`, "a container with no
`await`/`yield` inside is not lowered and still disposes inline" — the bug is that this
`using`-headed shape *should* count as having an await inside, via its nested `await using`
block, but the detection the `create_simple_machine` gate uses doesn't see it).

## 2. Files to touch

- `src/interpreter/generator_analysis.rs` — `scan_await_using`'s `Statement::ForOf` arm (split
  the `Using | AwaitUsing` match arm into two, relaxing only `Using`); update the two pinned
  scan tests (`suspendable_await_using_block_through_containers`,
  `lowering_that_would_flatten_a_lexical_scope_is_blocked`) to move the newly-isolatable shape
  and add a one-line comment anchoring why the `AwaitUsing` arm and the C-style `for` `Using` case
  stay as they are.
- `src/interpreter/generator_transform.rs` — new unit test(s) modeled on
  `test_plain_await_using_for_of_head_is_lowered` (already in this file) asserting
  `transform_async_function`/`transform_generator` pick the real state machine (not
  `create_simple_machine`) for the newly-isolatable shape.
- `test262-extra/` — new end-to-end tick-alignment regression(s), following the existing
  `await-using-for-of-head-dispose-tick-alignment.js` / `async-generator-...` pair's pattern and
  `observe()` helper exactly (one for a plain async function, one for an async generator, per the
  existing precedent of always covering both drivers since they share
  `transform_generator_inner_opts`).
- `docs/adr/` — no new ADR. This is a narrow extension of the existing gate logic documented in
  `ADR-2026-09-21-2015`/`ADR-2026-09-21-1007`, not a new mechanism or decision.
- `CONTEXT.md` — no new vocabulary; the fix doesn't introduce a new concept, only corrects which
  shapes an existing one (`create_simple_machine`'s lowering gate) applies to.
- Nothing under `scripts/`, `benchmarks/`, `.github/` — this is a pure engine/`src/` change.

Out of scope, not touched: `src/interpreter/exec.rs`, `src/interpreter/eval.rs`'s Annex-B-related
code, anything implementing `transform_for_statement`'s `CopyForward` (the C-style `using`-head
bug stays unfixed and gets its own follow-up issue per §0).

## 3. TDD slices

1. **Red:** add `test_using_for_of_head_wrapping_await_using_block_is_lowered` to
   `generator_transform.rs` (next to `test_plain_await_using_for_of_head_is_lowered`):
   `parse_fn_body("async function f(y) { for (using r of y) { { await using a = null; } } }")`
   through `transform_async_function`, asserting `sm.states.len() > 1`. Fails today (picks
   `create_simple_machine`, `states.len() == 1`).
   **Green:** in `generator_analysis.rs::scan_await_using`, change the `ForOf` arm's
   `ForInOfLeft::Variable(decl) if matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing)`
   branch into two: `VarKind::AwaitUsing => body.blocked_unless_none()` (unchanged) and
   `VarKind::Using => body.blocked_if(contains_annexb_function_declaration(&f.body))` (matching
   the existing `Let`/`Const` treatment three lines up, justified because
   `eval.rs::for_of_head_lexical` already treats `Using` identically to `Const` for per-iteration
   environment purposes — confirmed by reading that function — so there is no analogous
   "`CopyForward` doesn't understand `using`" risk for for-of the way there is for C-style `for`).
2. **Red:** update `generator_analysis.rs`'s existing unit tests: move
   `"for (using r of y) { { await using a = null; } }"` from
   `lowering_that_would_flatten_a_lexical_scope_is_blocked`'s `blocked` list to
   `suspendable_await_using_block_through_containers`'s `isolatable` list. Fails until step 1's
   production change lands (both tests are red/assert-flipped together with slice 1, so land them
   in the same commit as slice 1's production change — this is pinning the same behavior two
   ways, not a separate slice).
3. **Red:** `test262-extra/await-using-for-of-head-sync-using-wraps-await-using-dispose-tick-alignment.js`
   (plain async function) — copy the `observe()`/`asyncTest` harness from
   `await-using-for-of-head-dispose-tick-alignment.js`, scenario: `for (using r of [disposable])
   { { await using a = disposable2; } }`, asserting the tick order matches node's (derive it by
   hand from the two abstract operations in §1, or capture it from a node run — node has no
   known quirk for the sync-for-of-`using`-head case, only the C-style-for case, so node's output
   is trustworthy here; this was spot-checked already and matches node exactly once lowering is
   forced by an unrelated `await 0`, which is exactly the scenario this test forces to happen
   *without* an unrelated trigger). **Green:** no further production change — slice 1 already
   fixes it; this is the end-to-end proof.
4. **Red:** `test262-extra/async-generator-await-using-for-of-head-sync-using-wraps-await-using-dispose-tick-alignment.js`
   — same scenario inside an async generator (`async function*`, no `yield` before the loop, per
   the existing `async-generator-await-using-for-of-head-dispose-tick-alignment.js` pattern),
   since `transform_generator_inner_opts`'s gate is shared by both drivers (confirmed by reading
   `transform_generator`/`transform_async_function`'s call sites). **Green:** same, no further
   production change expected; if this one doesn't go green from slice 1 alone, that's new
   information requiring a return to investigation before touching more code.
5. **Pin item 1b and item 2 as regressions**, since both were found already-fixed and the
   investigation found no open test gap, but neither currently has a test for the *combination*
   this issue specifically asked about:
   - `generator_analysis.rs`: add `"for (await using r of y) { { await using a = null; } }"` to
     `suspendable_await_using_block_through_containers`'s `isolatable` list — wait, this is
     **not** correct: the static scan still (correctly, per §0) reports this `Blocked`. Instead,
     add a new end-to-end test262-extra regression (not a scan-level unit test, since the scan
     *should* stay `Blocked` here) asserting the runtime tick order for
     `for (await using r of y) { { await using a = null; } }` inside a plain async function
     matches node — this is the proof that `Blocked`-but-independently-forced-lowering is safe,
     guarding against a future refactor accidentally removing the independent
     `stmt_contains_await_using_head` gate without noticing this shape still depends on it.
   - No new switch test: `#853`'s own test plan already added exactly this combination
     end-to-end (`await-using-switch-case-block-dispose-tick-alignment.js`'s extension,
     per that PR's description) and it's running green today.

## 4. Test surface

- Targeted test262 run (no behavior there should move, this only affects async
  function/generator lowering of a resource-management shape not covered elsewhere):
  `uv run python scripts/run-test262.py test262/test/language/statements/for-of/
  test262/test/language/statements/for/ test262/test/staging/explicit-resource-management/
  --baseline-ref origin/main` (the last path only if present in this test262 pin; staging ERM
  tests are run explicitly per `AGENTS.md`).
- `test262-extra/` is the right home for the new regressions (not `tests/`): this is
  spec-correct, test262-style, ERM-specific behavior not covered by any existing test262 file,
  following the exact `esid`/`info:`/`asyncHelpers.js` pattern already established by
  `await-using-for-of-head-dispose-tick-alignment.js`.
- `cargo test --release` covers the new `generator_transform.rs`/`generator_analysis.rs` unit
  tests.
- Full `uv run python scripts/run-test262.py` (whole suite) before opening the PR, per `AGENTS.md`
  — this changes a shared lowering-decision gate (`create_simple_machine`), so any regression
  would most likely show up broadly, not just in ERM-tagged tests.
- `uv run python scripts/run-custom-tests.py` and `./scripts/lint.sh` as usual quality-gate steps
  (run as separate commands, not `&&`-chained, per the user's global instructions).

## 5. Regression risk

- **Primary risk:** `scan_await_using`'s `ForOf` `Using` arm now returns `Isolatable` more often.
  `has_suspendable_await_using_block`/`is_self_contained_isolatable` and
  `scan_scoped_list`'s `reaches_via_unsafe_flatten` check both consume this. Traced: `is_self_
  contained_isolatable` does *not* special-case a `Using` for-of head via this path (only
  `AwaitUsing`'s `disposes_at_head()`), so a `Using`-headed for-of next to a lexical sibling in an
  enclosing block now participates in `scan_scoped_list`'s sibling-safety fold the same way a
  `let`/`const`-headed one already does — this is the same code path already exercised by the
  existing `"for (let x of y) { await using a = null; }"`-style isolatable tests, so no new
  codepath, just a new input reaching it.
  - Mitigation: slice 2's test update exercises exactly this.
- **`transform_for_in_of_loop`/`ForOfHead` terminator itself is unchanged** — confirmed by
  reading it end-to-end: it already branches on `decl.kind` generically
  (`VarKind::Using | VarKind::AwaitUsing` both hit `add_disposable_resource`/`DisposeHint::
  for_var_kind`), so making the function lower more often doesn't exercise new runtime logic,
  only runs *existing, already-correct* logic on more inputs.
- **Baseline movement:** this should only ever add new test262 passes (a shape that previously
  ran via the tree-walker now runs via the lowered path, and both were already spec-correct for
  everything *except* this disposal-timing edge case) — not regress any. If `run-test262.py`
  shows a new *failure* anywhere outside `test262-extra/`, that's a signal the `Using` relaxation
  is unsafe in a shape this plan didn't probe, and is cause to stop and re-investigate rather than
  broaden the fix further.
- **Shared machinery touched:** `generator_transform.rs::transform_generator_inner_opts`'s
  `create_simple_machine` gate is on the hot path for every async function/generator body
  compile, not just ERM ones — but the change only adds a now-`true`-instead-of-`false` result
  for a `using`-for-of-head input the gate already had to evaluate; no new traversal is added for
  inputs that don't contain a `using`-headed for-of at all (short-circuiting `&&`/`||` chain,
  unchanged for every other shape).
- **Not touched, so no risk from this PR:** the bytecode VM (this is tree-walker/generator-
  transform only — `GeneratorStateMachine` lowering has no bytecode-VM equivalent per
  `AGENTS.md`'s architecture notes), GC rooting (`dispose_resources`/`DisposeCursor` call sites
  are unchanged, only *which* inputs reach the already-rooted `ForOfHead` path changes), the
  Node-compat library harnesses (none of the pinned libraries use `using`/`await using`).

## 6. Audit disposition (issue's item 3), from a full read-only sweep of every named call site

Structurally safe, no action — await is syntactically unreachable or the site is dead/unreachable:
- `generator_runtime.rs`'s legacy `generator_next`/`generator_return`/`generator_throw`
  (lines ~272, 300, 321) — dead code; every sync-generator call site constructs
  `IteratorState::StateMachineGenerator` instead, confirmed by grepping for any remaining
  constructor of the old variant (none found).
- `generator_next_state_machine_impl`'s ~11 `dispose_resources` call sites — sync `function*`
  only; the function itself asserts `unreachable!` on an `Await` terminator or a parking dispose
  attempt, and `await`/`await using` cannot parse inside a plain generator body.
- `generator_runtime.rs:1747`'s `ForOfHead` blocking dispose — the sync-generator sibling of the
  already-fixed async-generator `ForOfHead` at `generator_runtime.rs:5202`; safe for the same
  "no `await` in `function*`" reason.
- `dispose_env_for_for_of_unwind`'s blocking branch (`generator_runtime.rs:6962`) — only ever
  reached with `can_park=false`, which only the sync-generator routing helpers pass.
- `close_for_of_loop` (`eval.rs:9845`), all 3 call sites — only reached from
  `ArrayPatternIterOp::Finish` (destructuring-pattern iterator exhaustion), which never pushes a
  disposable resource (`using`/`await using` can't appear in a destructuring target).
- `unwind_async_for_of_loops` (`eval.rs:9969`) — already the *correct* reference implementation
  (parks via `DisposeCursor`/`ForOfUnwindOutcome::Parked`), not a gap.
- `exec.rs:1073`'s plain `Block` dispose — guarded by the existing `suspendable_dispose_block`
  mechanism; the blocking fallback is only reached for genuinely-safe or deliberately-`Blocked`
  (see next item) blocks.

Confirmed real, but explicitly out of scope for this PR (each needs its own follow-up, not a
bundled fix here, per "many small changes beat one large change"):
- **Annex-B function-declaration-sibling blocking** (`exec.rs:1073`, `2605`, `1961`/`2028`/`2030`,
  `2726`-`2769`) — real, but already tracked as `#842`, which a peer agent session is actively
  working on concurrently in this same codebase as of this writing. Not touched here; filing a
  duplicate or overlapping fix would conflict with that in-flight work.
- **Module-top-level-`await` blocking** (`exec.rs:2509`'s classic/non-lowered `for-of`/`for-await`
  tree-walker path, reached via the engine's existing, already-fully-blocking top-level-`await`
  mechanism, `eval.rs:10164`) — pre-existing, accepted design for top-level await generally, not
  specific to ERM disposal; out of this issue's theme.
- **C-style `for (using r = …; ; )` head's own scoping/disposal-timing** — see §0's "bigger bug
  found along the way." Needs `transform_for_statement` changes (or a `Using`-extended
  `await_using_for_head_scope` gated by a new, separate predicate — never by widening
  `ForStatement::disposes_at_head()` itself, which other code keys on for a narrower meaning),
  which is the "distinct, larger change" the issue's own text already anticipated. The
  implementation stage files a new issue for this (see §0) rather than attempting it here.

## 7. Out of scope

- Any change to `src/interpreter/exec.rs` (Annex-B paths, module top-level await, the C-style
  `using`-for-head tree-walker fallback) — see §6.
- `transform_for_statement`'s `CopyForward` mechanism and any `Using`-extension of
  `await_using_for_head_scope` — follow-up issue, not this PR.
- Widening `ForStatement::disposes_at_head()`'s definition — it has a narrower, load-bearing
  meaning elsewhere (`stmt_has_suspension`/`stmt_contains_await_using_head`); any C-style-`for`
  `Using` fix needs its own, separate predicate.
- Relaxing the `AwaitUsing` arm of `scan_await_using`'s `ForOf` case — proven unnecessary (§0);
  touching it risks the transform trying to isolate a shape that's already handled by the
  independent `stmt_contains_await_using_head` gate, for no behavioral gain.
- Any Annex-B hoisting work (`contains_annexb_function_declaration` and its guards) — `#842` is
  in flight on a peer session as of this writing; do not touch that code from this branch.
- Rewriting or re-deriving `test262-pass.txt` — read-only from `origin/main` per `AGENTS.md`.
- Bumping the `spec/` submodule pin to pick up Explicit Resource Management text — a separate,
  deliberate operation, not a side effect of this bugfix.
