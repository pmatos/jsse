# Plan: issue #665 — `await using` disposal still drains microtasks inline in several shapes

## 0. Current state vs. the issue text (read this first)

The issue's own headline repro (`try { { await using a = null; throw 1; } } catch...`)
**already passes** on this branch — built and ran it against `node` (the reference
engine), byte-identical output. `while` bodies, `switch` case blocks (with a nested
`{ }`), `for (var ...)` and `for-of (const ...)` bodies, and top-level module
`await using` also already match `node`. `#688`, `#699`, `#701` and `#703` fixed
these. This plan does not re-fix what's already fixed; it targets the shapes that
are still broken, confirmed by running both engines side by side (`node` as
oracle, never as spec authority — see `CLAUDE.md`'s authority order):

| Shape | Status |
|---|---|
| `try { { await using a=null; throw 1; } } catch {}` | **passes** |
| `switch` case with nested `{ await using }` block | **passes** |
| `while` / `do-while` body with `await using` | **passes** |
| `for (var i...)` / `for-of (var x of ...)` body | **passes** |
| module top-level `await using a = null;` (no wrapping container) | **passes** |
| `for (let i...)` body with `await using` | **BROKEN** |
| `try`/`switch`/plain-block body with a **lexical sibling** (`let y=1;`) next to a nested `await using` block | **BROKEN** (all three containers) |
| `for (var k in {...})` body with `await using` | **BROKEN** |
| async generator, `await using` directly in a nested `{ }` block | **BROKEN** |
| async generator, `await using` directly in the function's own top-level body (no block at all) | **BROKEN** |
| async generator, `try { { await using } }` or `for (let...)` body | **BROKEN** |

"Broken" means: a job already queued *before* the async call runs (a witness
microtask chain) fires *before* the synchronous caller's own next statement,
i.e. the engine drained the job queue inside the call instead of suspending it
— the exact symptom the issue names. Repros live under `$TMPDIR/jsse665/*.js`
in this workspace (not committed; recreate from the shapes above if needed).

This plan fixes the **two confirmed, well-isolated regressions** that explain
nearly every broken row above with one root cause each, for a scoped, reviewable
PR. The remaining broken rows (async-generator function-level disposal, and
Try/Switch/For containers inside async generators) need a second, larger
architectural piece and are left as follow-up (§7).

## 1. Problem restated

`DisposeResources`' `Await`s must suspend the running execution context and let
the job queue drive the resumption (per `Await`, spec id `await`), the same way
every other `await` in async code does. For two specific shapes, jsse's
generator-transform classifies the surrounding statement as "doesn't need its
own state" and lets the tree-walker run the whole container (loop iteration,
`try` clause, etc.) in one shot; when the nested `await using` block's disposal
pops that container's `DisposeCursor`, there is no state boundary left to
suspend at, so the interpreter falls back to `Interpreter::await_value`'s
blocking path, which drains the job queue inline before returning control to
the synchronous caller. Both shapes trace back to the same place: an
over-conservative `Blocked` classification in
`generator_analysis.rs::scan_await_using`, written before `#703` gave every
lowered block/loop/catch its own per-entry scope (`ScopeAction::OpenBlock` /
`CopyForward`). That per-entry scoping now already protects the exact lexical
scope these `Blocked` arms were guarding, making the conservatism stale.

## 2. Spec basis

The pinned `spec/` snapshot (tc39/ecma262 @ `270a490b`) does **not** contain
Explicit Resource Management (`using`/`await using`/`DisposeResources`) at all —
confirmed by grep (`Disposable`, `AwaitUsingDeclaration`, `DisposeResources`:
zero matches). This matches the existing `test262-extra/await-using-*` files in
this repo, which already cite `esid: sec-disposeresources` as an *external*
anchor (the Explicit Resource Management proposal text, test262 `features:
[explicit-resource-management]`), not a clause inside this submodule. This plan
follows that existing convention and does the same.

Clauses actually present in `spec/spec.html` that govern the defect:

- **`Await`** (abstract operation, id `await`, `spec.html:51047`, oldids
  `await-fulfilled`/`await-rejected`): defines `Await(value)` as scheduling its
  continuation as a job and suspending the running execution context. jsse's
  `Interpreter::await_value` (`src/interpreter/eval.rs:9890`) implements this
  correctly when it *can* suspend; the bug is call sites that reach it from a
  context with no state boundary to suspend *to*, so they fall back to draining
  the job queue inline instead.
- **`sec-forbodyevaluation`** / **`sec-createperiterationenvironment`**
  (`spec.html:22070`, `22100`): a `for (let ...)` loop creates a fresh
  per-iteration environment each pass. This is exactly what `#703`'s
  `ScopeAction::CopyForward` implements, and exactly the mechanism the stale
  `Blocked` classification was written to protect *before* `CopyForward`
  existed.
- **`sec-block`** / **`sec-blockdeclarationinstantiation`** (`spec.html:21341`,
  `21412`): a `Block` gets a fresh declarative environment per entry — the
  general case `ScopeAction::OpenBlock` (`#703`) implements for any
  state-machine-lowered container (loop body, `try`/`catch`/`finally` clause).
- **`sec-try-statement-runtime-semantics-evaluation`** /
  **`sec-runtime-semantics-catchclauseevaluation`** (`spec.html:23182`,
  `23150`): each `try`/`catch`/`finally` clause is itself a `Block` production,
  confirming the same per-entry-scope reasoning applies to `try` clause bodies
  with a lexical sibling next to an `await using` block.
- **`sec-runtime-semantics-forinofheadevaluation`** /
  **`sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset`**
  (`spec.html:22352`, `22388`): governs `for-in` body evaluation; nothing in it
  requires blocking `await using` disposal in a `for-in` body any more than in
  a `for-of` body (already fixed).

External (not in `spec/`, implemented against the proposal text per existing
codebase convention, `esid: sec-disposeresources`): `DisposeResources`'
trailing/disposer `Await` steps — the thing every one of these call sites must
suspend at instead of draining.

## 3. Files to touch

- `src/interpreter/generator_analysis.rs` — relax the stale `Blocked` arms in
  `scan_await_using`/`scan_flattened_list` (the `declares_lexical_binding`
  downgrade, the `Statement::For` lexical-init arm, the `Statement::ForIn` arm).
  Update the doc comments on `AwaitUsingScan::Blocked` and `scan_flattened_list`
  that currently justify the conservatism being removed.
- `src/interpreter/generator_transform.rs` — no behavioral change expected
  (the existing `transform_scope_block`/`ScopeAction::OpenBlock` paths already
  handle every container kind correctly once `scan_await_using` stops blocking
  them), but add/adjust unit tests in its `#[cfg(test)]` module alongside
  `has_suspendable_await_using_block`'s existing coverage.
- `test262-extra/await-using-loop-body-dispose-tick-alignment.js` — add the
  `for (let ...)` body case (the file already covers `for (var ...)`, `for-of`,
  `while`, `do-while`; the `let` variant is conspicuously absent — that gap is
  the bug).
- `test262-extra/await-using-try-catch-finally-dispose-tick-alignment.js` and
  `test262-extra/await-using-switch-case-block-dispose-tick-alignment.js` — add
  the lexical-sibling-next-to-a-nested-block case to each.
- New `test262-extra/await-using-for-in-body-dispose-tick-alignment.js` (no
  existing file covers `for-in`).
- `src/interpreter/eval/generator_runtime.rs` — async generator state-stepping
  loop (the `exec_state_machine_body` call around what is currently line
  `~4229`, parallel to the plain-generator one around `~792`): mirror the
  `suspendable_dispose_block` / `parked_block_dispose` wiring that
  `src/interpreter/eval.rs:~8916-8939` already does for the async-function
  driver, so a directly-nested `await using` block in an async generator parks
  its cursor instead of calling the blocking `dispose_resources`.
- New `test262-extra/await-using-async-generator-block-dispose-tick-alignment.js`
  — the `observe()`-witness-chain pattern from
  `await-using-loop-body-dispose-tick-alignment.js`, adapted to drive an async
  generator with `for await` and assert tick alignment for a directly-nested
  `{ await using a = null; }` block in the generator body.
- `CONTEXT.md` — no new vocabulary; `ScopeAction::OpenBlock`/`CopyForward`,
  `suspendable_dispose_block`, and `PendingDispose`/`DisposeThen` are already
  documented there from `#645`/`#688`/`#699`/`#703`.
- No `docs/adr/` entry: this follows the established architecture
  (`PendingDispose`/`DisposeCursor`/`ScopeAction`) with no new decision to
  record; it closes a gap the existing per-entry-scope decision (`#703`) left
  behind.

## 4. TDD slices

1. **Red:** add the `for (let i = 0; i < 2; i++) { await using a = null; ...}`
   case to `await-using-loop-body-dispose-tick-alignment.js`, expected array
   `['b0', 'sync-end', 'w1', 'b1', 'w2', 'after', 'w3', 'settled', 'w4']`
   (mirroring the existing `for (var ...)` case's shape — derive the exact
   array by running the same shape against `node` first, then cross-check
   against the `DisposeResources` Await-count model already used by the
   sibling cases in that file; `node` is corroboration, not authority).
   Run it — confirms it fails today (`run-test262.py test262-extra/...`).
   **Green:** in `generator_analysis.rs`, drop the `Statement::For` arm's
   `body.blocked_unless_none()` downgrade for a lexical (`let`/`const`) init
   when the body is otherwise `Isolatable` (so it resolves to `Isolatable`
   directly, same as `Statement::While`'s arm already does). Re-run; also run
   `await-using-lowering-preserves-block-scope.js` (the closure-per-iteration
   regression guard) to confirm `#703`'s `CopyForward` still gives each
   iteration its own binding once the body is state-machine-lowered via this
   path.
2. **Red:** add a `for (let i …) { fns.push(() => i); { await using a = null; } }`
   case to `await-using-lowering-preserves-block-scope.js` if slice 1's fix
   doesn't already exercise a closure capturing the per-iteration binding
   (check first; the file's existing `for (let i...)` case may already cover
   this once slice 1 routes it through `transform_yielding_statement`).
   **Green:** none expected beyond slice 1 if `CopyForward` already composes
   correctly with `transform_scope_block`; this slice is a verification step,
   not a new fix, unless the red run surfaces a second bug.
3. **Red:** add the lexical-sibling case to
   `await-using-try-catch-finally-dispose-tick-alignment.js` (`try { let y = 1;
   { await using a = null; L('t'); } } catch (e) {}`) and to
   `await-using-switch-case-block-dispose-tick-alignment.js` (`case 1: let y =
   1; { await using a = null; L('s'); } break;`), with expected arrays derived
   the same way as slice 1 (run against `node`, pin the array). Confirm both
   fail today.
   **Green:** in `generator_analysis.rs::scan_flattened_list`, drop the
   `declares_lexical_binding` downgrade to `Blocked` (the whole `if combined ==
   AwaitUsingScan::Isolatable && stmts.into_iter().any(declares_lexical_binding)
   { Blocked }` branch becomes dead weight once `ScopeAction::OpenBlock`
   guarantees the flattened list's own lexical declarations get a fresh
   per-entry environment regardless of whether a nested block is pulled into
   its own state). Re-run both new cases plus
   `await-using-lowering-preserves-block-scope.js`'s `try` case (already
   covers a `let` shadowing an outer binding) and the full
   `generator_analysis.rs`/`generator_transform.rs` unit test suites
   (`cargo test --release`).
4. **Red:** add `await-using-for-in-body-dispose-tick-alignment.js` (new file,
   same `observe()` pattern, `for (var k in {a:1,b:2}) { await using a = null;
   L('k'+k); }`), confirm it fails today.
   **Green:** in `generator_analysis.rs`, drop the `Statement::ForIn` arm's
   `blocked_unless_none()` (same change as slice 1, different arm). Re-run,
   plus the `for-in` case already present in
   `await-using-lowering-preserves-block-scope.js` (visits every key).
5. **Red:** add
   `await-using-async-generator-block-dispose-tick-alignment.js`: an async
   generator with `async function* gen() { { await using a = null; L('in-gen');
   } L('after-block'); }`, driven via `for await` from an async function,
   asserting the witness chain interleaves around the block's disposal the
   same way the function-level case does. Confirm it fails today (drains
   inline — the symptom reproduced manually this session).
   **Green:** in `src/interpreter/eval/generator_runtime.rs`, before calling
   `exec_state_machine_body` in the async-generator state-stepping loop,
   compute `isolated_block` from `state_machine.states[current_id].body`'s
   last statement exactly as `src/interpreter/eval.rs` already does for the
   async-function driver, swap it into `self.suspendable_dispose_block` for
   the call, and route a `self.parked_block_dispose.take()` result into
   whatever this driver's equivalent of `PendingDispose`/`continue` dispatch
   is (likely a new local `pending_dispose` branch parked the same way the
   async-function loop parks `DisposeThen::Block`, or an immediate call into
   `async_gen_await_resume` — confirm the exact shape by reading
   `async_gen_await_resume`, `src/interpreter/eval/generator_runtime.rs:5961`,
   before wiring this). Re-run the new test plus the full test262-extra suite
   and `cargo test --release` (this touches shared generator-runtime code
   paths used by every generator, not just async ones with `await using`).

Run the full project quality gate after each slice, as separate commands per
`CLAUDE.md` (never `&&`-chained): `cargo build --release`, `cargo test
--release`, `./scripts/lint.sh`, `uv run python scripts/run-test262.py` (full
suite, baseline from `origin/main:test262-pass.txt` — do **not** pass
`--update-baseline`), `uv run python scripts/run-custom-tests.py`.

## 5. Test surface

- `test262/test/language/statements/using/`,
  `test262/test/language/statements/await-using/`,
  `test262/test/language/statements/for-await-of/`,
  `test262/test/language/statements/try/`,
  `test262/test/language/statements/for/`,
  `test262/test/language/statements/for-in/`,
  `test262/test/language/statements/switch/`,
  `test262/test/language/statements/async-generator/`,
  `test262/test/language/expressions/async-generator/` — run targeted; none of
  these assert exact microtask tick alignment (confirmed: the two async-
  generator `await-using` tests under `language/statements/await-using/` pass
  today despite the bug — they check *that* disposal happens, not *when*), so
  they will not regress and will not catch this class of bug either. That's
  what the new `test262-extra` files are for.
- `test262-extra/await-using-*-tick-alignment.js` (existing + new files listed
  in §3/§4) is the actual regression surface for this change. Run via
  `uv run python scripts/run-test262.py test262-extra/` (per `CLAUDE.md`, no
  dedicated runner).
- `test262-extra/await-using-lowering-preserves-block-scope.js` is the guard
  against over-relaxing `scan_await_using`: it must keep passing after every
  slice, since it specifically pins per-iteration/per-entry lexical scoping for
  the containers this plan is relaxing.
- `cargo test --release` for `generator_analysis.rs`'s and
  `generator_transform.rs`'s own unit tests (`has_suspendable_await_using_block`,
  `suspendable_await_using_block_through_containers`, and friends) — extend
  `suspendable_await_using_block_through_containers` with cases for the
  relaxed shapes (`for (let...)`, lexical sibling, `for-in`) so the classifier
  change has a direct unit-level pin, not just an end-to-end tick-alignment
  test.
- Full `uv run python scripts/run-test262.py` (baseline from
  `origin/main:test262-pass.txt`) after every slice — this change touches the
  generator-transform classifier used by every async function/generator in
  the suite, so a full run (not just a targeted directory) is required before
  calling any slice done.

## 6. Regression risk

- **Highest risk:** `generator_analysis.rs::scan_await_using` is consulted
  (via `has_suspendable_await_using_block`, gated on `detect_for_await`, i.e.
  plain async functions only — see §7) for *every* `try`/`for`/`for-in`/
  `switch`/loop statement in every async function, not just ones with `await
  using`. Relaxing `Blocked` → `Isolatable` changes which containers get
  lowered into `transform_scope_block`'s `EnterScope`/`ExitScope` state
  machinery versus the plain tree-walker. A mistake here silently changes
  closure/TDZ/shadowing behavior for code that has nothing to do with
  disposal — exactly what
  `await-using-lowering-preserves-block-scope.js` exists to catch, which is
  why it's named explicitly as a required re-run after every slice, not just
  the matching one.
- **Generator-transform hot path:** `scan_await_using` runs during every
  async-function transform (it's part of the `create_simple_machine`
  fast-path check at `generator_transform.rs:541-556`), so a classification
  bug here can silently flip simple (non-state-machine) async functions into
  the state-machine path or vice versa, affecting performance-sensitive code
  far beyond `await using` users. The full test262 run (not a targeted
  subdirectory) is the only thing that would catch a regression in, e.g., a
  `for-in` loop that has nothing to do with disposal.
- **`src/interpreter/eval/generator_runtime.rs`** is shared by plain
  generators, async generators, and `yield*` delegation to async iterables —
  it's one of the densest files in the interpreter (confirmed: ~20
  `dispose_resources` call sites spread across generator/async-generator/
  yield* paths). Slice 5 touches only the async-generator state-stepping loop
  and must not change behavior for the plain-generator loop (which has its own,
  separate, `is_async: false` copy of the same loop shape a few hundred lines
  away) or for `yield*`.
- **GC rooting:** this plan's slice 5 reuses the *existing*
  `parked_block_dispose`/`DisposeCursor` rooting path (already traced via
  `gc_root_scope`/`gc_safepoint` for the async-function driver), so no new GC
  surface is introduced here. The deferred function-level async-generator
  slice (§7) *would* need a new field and new GC tracing (precedent:
  `bf0d87f3`, "root the parked cursor while wiring the Await continuation") —
  flagged there, not here, since it's out of scope for this PR.
- **Bytecode fast path:** `bytecode/` is feature-flagged off by default and
  this plan does not touch it; `async`/`await using` function bodies already
  go through the tree-walker/state-machine path exclusively as far as this
  investigation found, so no interaction expected, but worth a grep-confirm
  during implementation if `bytecode_enabled` is ever turned on for async code.

## 7. Out of scope (tracked as follow-up, not bundled into this PR)

File each of these as its own GitHub issue once this PR lands (per the
"many small changes" rule — do not bundle):

1. **Async generator function-level `await using` disposal** (`async
   function* gen() { await using a = null; ... }`, no wrapping block at all —
   confirmed broken, the single most basic case). This needs a new
   `pending_dispose`-equivalent field on the async generator's suspended state
   (`IteratorState::StateMachineAsyncGenerator`, `types.rs:~1472`), GC tracing
   for it (`gc.rs`, precedent `bf0d87f3`), and resumption plumbing through
   `async_gen_await_resume` (`generator_runtime.rs:5961`) for the `Return`/
   `Throw`/`Normal`-completion dispose call sites (confirmed at
   `generator_runtime.rs:~4175`, `~4265`, `~5809`, and others). Materially
   larger than this plan's slices; its own PR.
2. **`try`/`switch`/`for`-wrapped `await using` inside async generators**
   (confirmed broken: `async function* gen() { try { { await using a=null; } }
   catch {} }` and the `for (let...)` equivalent). Slice 5 in this plan only
   fixes a *directly*-nested block in an async generator; `Try`/`Switch`/`For`
   containers route through a different branch
   (`transform_try_statement`/`transform_for_statement`'s `ScopeAction::
   OpenBlock` path, which has no parking hook at all, unlike `Block`'s own
   fallback at `generator_transform.rs:950-968`). Needs that fallback
   generalized beyond `Block`, which depends on #1 existing first (both need
   the same new async-generator dispose-parking primitive).
3. **For-await-of early-exit tick ordering** (not an `await using` bug):
   `for await (const v of gen()) { break; }` where `gen`'s `finally` runs one
   tick earlier in jsse than in `node` (`iter-finally` before `w3` vs. after).
   Root cause looks unrelated to disposal — likely a missing `Await` in
   `AsyncGeneratorUnwrapYieldResumption` (`spec.html:50772`) on a return
   completion injected at a suspended `yield`. File separately; do not
   conflate with this issue.
4. **For-of/for-await head disposal audit** (`close_for_of_loop`,
   `unwind_async_for_of_loops`, `exec.rs`'s loop/switch `dispose_resources`
   call sites named in the issue). Every shape this investigation actually
   probed for these (for-of body, for-await `break`-triggered `IteratorClose`)
   already matched `node` or reduces to #3. Needs its own targeted probing
   pass (more shapes: `return()` instead of `break`, nested for-of loops,
   for-of over a sync iterable inside an async function) before claiming a
   fix is needed — do not guess at a fix for an unconfirmed bug.
5. **Remaining blocking `await_value`/`dispose_resources` callers** named in
   the issue (`exec.rs:2310`, the ~20 `dispose_resources(...)` sites in
   `generator_runtime.rs` beyond the ones #1/#2 above name) — audit once #1/#2
   land, since several of them are plain-generator or `yield*` paths that
   structurally cannot reach an `await using` disposal (no `await` is legal in
   a non-async generator), and auditing them before #1/#2 exist would be
   premature.
6. **Unify `has_block_with_await_using` (narrow) with
   `has_suspendable_await_using_block` (broad) at
   `generator_transform.rs:849`** — once #1/#2 give async generators a real
   parking primitive for `Try`/`Switch`/`For`, the narrow check becomes
   redundant with the broad one everywhere, not just under `detect_for_await`.
   Pure cleanup, deferred until the behavior it would unify actually exists on
   both sides.

A `gh issue comment 665` documenting this split (what's already fixed, what
this PR closes, what's deferred and why) will be posted alongside this commit,
per the operating contract's judgment-call documentation requirement.
