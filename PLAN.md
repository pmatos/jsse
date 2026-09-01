# Plan: issue #555 — import defer cannot evaluate a deferred module with an async cycle dependency

## 1. Problem restated

When a deferred module's dependency graph reaches back into a strongly-connected
component (SCC) whose *cycle root* is still `EVALUATING-ASYNC` (suspended on a
top-level `await`), but a *non-root member* of that same SCC has already finished
its own synchronous body and is individually `EVALUATED`, jsse's deferred-namespace
evaluation gate (`ReadyForSyncExecution`) and its dependency-gathering helper
(`GatherAsynchronousTransitiveDependencies`) both consult the non-root member's own
per-module status instead of asking the question the spec actually asks — is the
member's *cycle root* done? — and instead of asking whether the member's status is
*EVALUATING* specifically (as opposed to *EVALUATING-ASYNC*). Both misreads make the
engine either throw `TypeError: Cannot synchronously evaluate a module with
top-level await or that is currently being evaluated` when it should wait, or skip
scheduling the pending async dependency it should have waited on. Root cause is one
shared bug: `src/interpreter/mod.rs`'s SCC-finalization step marks *every* module
popped off the DFS stack as `evaluated` (or not) based on the **SCC root's** own
`async_evaluation_order`, instead of **each popped module's own**
`async_evaluation_order`, as the spec requires per-module.

## 2. Spec basis

### In `spec/` (mainline ecma262, pin `270a490b3f8bf6f15bced16021ee0c3ff107f823`)

- **`sec-innermoduleevaluation`** (`spec/spec.html:27182`), the `InnerModuleEvaluation`
  abstract operation. The SCC-finalization loop, steps corresponding to
  `spec/spec.html:27233-27243`:
  ```
  1. If module.[[DFSAncestorIndex]] = moduleIndex, then
    1. Let done be false.
    1. Repeat, while done is false,
      1. Let requiredModule be the last element of stack. Remove it.
      1. Assert: requiredModule.[[AsyncEvaluationOrder]] is either an integer or unset.
      1. If requiredModule.[[AsyncEvaluationOrder]] is unset, set requiredModule.[[Status]] to evaluated.
      1. Otherwise, set requiredModule.[[Status]] to evaluating-async.
      1. If requiredModule and module are the same Module Record, set done to true.
      1. Set requiredModule.[[CycleRoot]] to module.
  ```
  This is the exact clause the current jsse code violates: it must decide
  `evaluated` vs `evaluating-async` **per popped module, using that module's own
  `[[AsyncEvaluationOrder]]`** — not the SCC root's. This part of the fix is
  grounded entirely in mainline `spec/`, already vendored, unrelated to any
  proposal.

### Not in `spec/` — proposal-only AOs

`grep -n "IsModuleSCCEvaluated\|ReadyForSyncExecution\|GatherAsynchronousTransitiveDependencies" spec/spec.html`
returns **zero hits** at this pin. These three abstract operations belong to the
`import-defer` proposal (tc39/proposal-defer-import-eval), which is not yet merged
into mainline ecma262. jsse's original `import defer` implementation (commit
`7bc436e`, "feat: [US-024] - Implement `import defer`") was grounded the same way,
citing "the import-defer proposal" directly rather than a `spec/` clause, because
none existed. Following that precedent, this plan grounds the three AOs on the
verbatim algorithm text embedded in the failing test262 test's own `info:` field
(`test262/test/language/import/import-defer/evaluation-top-level-await/async-cycle-dependency-of-deferred-module/main.js`),
which is the closest available primary source and is itself a copy of the
proposal's normative text (per test262's `esid: sec-IsModuleSCCEvaluated` tag
convention of pointing at proposal spec sections when a proposal isn't merged):

```
IsModuleSCCEvaluated ( module )
  1. If module.[[CycleRoot]] is not EMPTY, then
    1. If module.[[CycleRoot]].[[Status]] is EVALUATED, return true.
    1. Return false.
  1. If module.[[Status]] is EVALUATED, return true.
  1. Return false.

GatherAsynchronousTransitiveDependencies ( module, [ seen ] )
  1. If seen is not specified, let seen be a new empty List.
  1. Let result be a new empty List.
  1. If seen contains module, return result.
  1. Append module to seen.
  1. If module is not a Cyclic Module Record, return result.
  1. If module.[[Status]] is either EVALUATING or IsModuleSCCEvaluated(module), return result.
  1. If module.[[HasTLA]] is true, then
    1. Append module to result.
    1. Return result.
  1. For each ModuleRequest Record required of module.[[RequestedModules]], do
    1. Let requiredModule be GetImportedModule(module, required.[[Specifier]]).
    1. Let additionalModules be GatherAsynchronousTransitiveDependencies(requiredModule, seen).
    1. For each Module Record m of additionalModules, do
      1. If result does not contain m, append m to result.
  1. Return result.

ReadyForSyncExecution ( module [ , seen ] )
  1. If module is not a Cyclic Module Record, return true.
  1. If seen is not present, set seen to a new empty List.
  1. If seen contains module, return true.
  1. Append module to seen.
  1. If IsModuleSCCEvaluated(module), return true.
  1. If module.[[Status]] is EVALUATING or EVALUATING-ASYNC, return false.
  1. Assert: module.[[Status]] is LINKED.
  1. If module.[[HasTLA]] is true, return false.
  1. For each ModuleRequest Record request of module.[[RequestedModules]], do
    1. Let requiredModule be GetImportedModule(module, request).
    1. If ReadyForSyncExecution(requiredModule, seen) is false, then
      1. Return false.
  1. Return true.
```

The key structural gap: `IsModuleSCCEvaluated` does not exist as a helper in jsse
today. `ready_for_sync_execution` and `gather_async_transitive_deps` both inline a
direct `module.evaluated` check where the AOs above call `IsModuleSCCEvaluated`,
which redirects through `[[CycleRoot]]` once a module has one. That redirection is
precisely what this issue is missing.

## 3. Files to touch

All production changes are confined to **`src/interpreter/mod.rs`**:

1. `inner_module_evaluation`'s SCC pop loop, ~`mod.rs:3879-3896` (currently computes
   one `has_async` flag from the frame's own `module` and applies it to every
   popped module — must instead read each `popped_mod`'s own
   `async_evaluation_order`).
2. New private helper `is_module_scc_evaluated(&self, path: &ModuleKey) -> bool`,
   placed near `gather_async_transitive_deps`/`ready_for_sync_execution`
   (~`mod.rs:4221`), implementing `IsModuleSCCEvaluated`.
3. `ready_for_sync_execution`, ~`mod.rs:4295-4332` (replace the direct
   `module_ref.evaluated` early-return with `is_module_scc_evaluated`).
4. `gather_async_transitive_deps`, ~`mod.rs:4221-4262` (replace the direct
   `module_ref.evaluated` early-return with a check that distinguishes literal
   `EVALUATING` from `EVALUATING-ASYNC`, per the invariant in §4 Slice 2).

No changes to `src/parser/`, `src/lexer.rs`, `src/interpreter/eval/modules.rs`
(`ensure_deferred_namespace_evaluation` already delegates to
`ready_for_sync_execution` and needs no changes itself), `src/interpreter/builtins/`,
or any `docs/adr/` — this is a targeted correctness fix to existing machinery, not
a new architectural decision. No `CONTEXT.md` vocabulary changes.

## 4. TDD slices

**Load-bearing invariant** (not directly spec text, but forced by the single write
site of `cycle_root`, confirmed by `grep -n "\.cycle_root" src/interpreter/mod.rs
src/interpreter/eval/modules.rs`: the only assignment is
`popped_mod.borrow_mut().cycle_root = Some(canon.clone())` inside the pop loop
itself, `mod.rs:3885`):

> `cycle_root.is_some()` ⟺ the module has been through the SCC pop loop at least
> once ⟺ its spec `[[Status]]` is `EVALUATED` or `EVALUATING-ASYNC`, **never**
> literal `EVALUATING`. Conversely `cycle_root.is_none() && is_evaluating` ⟺
> literal `EVALUATING` (still on some DFS stack, not yet finalized).

This lets `is_evaluating && cycle_root.is_none()` stand in for "`[[Status]]` is
literally `EVALUATING`" wherever the AOs need that specific distinction (Slice 2),
while plain `is_evaluating` (regardless of `cycle_root`) still means "`EVALUATING`
**or** `EVALUATING-ASYNC`" wherever the AOs treat those two the same (Slice 3).

1. **(Optional, red) Cheap regression harness.** Add
   `tests/import_defer_async_cycle.rs`, following the existing
   `tests/module_source_host_specifier.rs` pattern (spawn
   `env!("CARGO_BIN_EXE_jsse")` against a scratch dir via `--module`, no test262
   harness needed). One scenario: a trimmed version of the issue's fixture graph
   (A has TLA and cycles with B; C imports Middle then resolves A's blocker; Middle
   `import defer`s D, whose only dependency is B) with a plain
   `globalThis.evaluations` array and a final `if (...) throw new Error(...)`
   instead of `assert.compareArray`/`$DONE`. Assert the process exits successfully
   with the same 8-element order as the test262 test. This is a nice-to-have for
   fast iteration, not a substitute for the test262 gate in Slice 5 below — if
   reproducing the TLA/microtask-drain timing through the CLI turns into a fight
   with fixture plumbing, drop it and rely on Slice 5 alone.

2. **(Fix 1) SCC pop loop uses each popped module's own async status.** In
   `inner_module_evaluation`'s pop loop (`mod.rs:3879-3896`), remove the single
   `has_async` variable computed from `module` (the root) and instead, for each
   `popped_mod`, check *that module's own* `async_evaluation_order.is_some()` to
   decide whether to set `evaluated = true; is_evaluating = false` (own order
   `None` → spec `evaluated`) or leave it alone (own order `Some` → spec
   `evaluating-async`). `cycle_root` assignment (already unconditional and
   already correct) is untouched.
   - **Checkpoint (not yet the full fix):** after this slice alone, re-run the
     failing test262 scenario — it still fails with the same `TypeError`, because
     `ready_for_sync_execution`/`gather_async_transitive_deps` still consult
     `B.evaluated` directly instead of redirecting through `B.cycle_root`. But the
     module-state fields are now individually correct: for the issue's fixture,
     `B.evaluated == true`, `B.is_evaluating == false`, `A.evaluated == false`,
     `A.is_evaluating == true`, both `B.cycle_root == Some(A)` and
     `A.cycle_root == Some(A)`.
   - Production code only, no new test file is needed for this slice — it's an
     internal-state correction observable only via the following slices' behavior.

3. **(Fix 2 + 3) `IsModuleSCCEvaluated` helper wired into `ReadyForSyncExecution`.**
   Add `is_module_scc_evaluated` and use it in `ready_for_sync_execution`
   (`mod.rs:4295-4332`), replacing the direct `module_ref.evaluated` check, checked
   *before* the `is_evaluating`/`has_tla` checks (matching AO step order: step 5
   before step 6). Note for the implementer: the AO checks `[[Status]]` only, never
   `[[EvaluationError]]` — `ready_for_sync_execution` returning `true` for an
   errored cycle root is spec-faithful; the error itself surfaces separately via
   `eval/modules.rs:216-220` (already-evaluated branch) and the top-of-function
   error propagation in `inner_module_evaluation`. Don't add an error check the AO
   doesn't have.
   - **Checkpoint:** the test262 scenario still fails with the same `TypeError` —
     `ready_for_sync_execution(D)` still correctly returns `false` while A is
     suspended (now via `is_module_scc_evaluated(B)` redirecting to `A.evaluated ==
     false`, rather than the old, coincidentally-also-`false` direct read of
     `B.evaluated`). This slice is a no-observable-change refactor *for this
     specific test* in isolation, but it's the one that makes `ReadyForSyncExecution`
     correct in general (e.g. for cases where `B.evaluated` would have been
     wrongly `true`).

4. **(Fix 4) `GatherAsynchronousTransitiveDependencies` distinguishes `EVALUATING`
   from `EVALUATING-ASYNC`.** In `gather_async_transitive_deps` (`mod.rs:4221-4262`),
   replace the direct `module_ref.evaluated` early-return with:
   `(module_ref.is_evaluating && module_ref.cycle_root.is_none()) ||
   self.is_module_scc_evaluated(&canon)`. This is the slice that actually flips the
   test262 assertion, because `gather_async_transitive_deps` (unlike
   `ready_for_sync_execution`) must *not* skip a module that's `EVALUATING-ASYNC` —
   it needs to walk into it and, if `HasTLA`, append it, so the caller
   (`inner_module_evaluation`'s `evaluation_list` construction) treats it as a
   pending async dependency of the deferring module.
   - **Checkpoint (green):** `gather_async_transitive_deps(D)` now yields `[A]`
     instead of `[]`. Middle's `evaluation_list` includes A, so Middle acquires
     `pending_async_dependencies == 1` against A and becomes `EVALUATING-ASYNC`
     itself instead of running its body synchronously and immediately. Middle's
     body is deferred until `async_module_execution_fulfilled(A)` walks
     `gather_available_ancestors` and finds Middle ready. The full 8-element
     `globalThis.evaluations` order now matches, and the test262 scenario passes.

5. **(Regression gate) Targeted test262 re-run**, see §5.

## 5. Test surface

Targeted test262 directories (run via `uv run python scripts/run-test262.py
<dir>`), all currently green except the one file this issue is about:

- `test262/test/language/import/import-defer/` — 108/109 passing today (only
  failure is this issue's file); this is the primary gate. Three files in this
  directory are the ones most likely to interact with Fix 4's new skip condition
  (a literally-`EVALUATING` module reached through a deferred edge), so re-run
  them explicitly and confirm they still pass, not just "directory total didn't
  drop":
  - `import-defer/errors/get-other-while-dep-evaluating-async/main.js`
  - `import-defer/errors/get-other-while-evaluating-async/main.js`
  - `import-defer/evaluation-top-level-await/sync-dependency-of-deferred-async-module/main.js`
- `test262/test/language/module-code/top-level-await/` — 253/253 passing today
  (baseline confirmed this session). Fix 1 touches the SCC pop loop used by *every*
  cyclic module evaluation, not just deferred ones, so any async-cycle TLA test
  lives here.
- `test262/test/language/expressions/dynamic-import/` — 1900/1900 passing today
  (baseline confirmed this session). Covers `import()`/`import.defer()` dynamic
  paths, which share `gather_async_transitive_deps` via
  `evaluate_async_transitive_deps` (`mod.rs:4206-4218`).

No `test262-extra/` addition is needed: test262 already exercises this exact
scenario end-to-end (that's the issue's own repro), so there's no spec-correct
behavior left uncovered that would justify a duplicate. The optional
`tests/import_defer_async_cycle.rs` from Slice 1 is a `cargo test`-speed
convenience, not a coverage gap-filler.

After the targeted directories are green, run the full suite per CLAUDE.md
("After any implementation work, run the full test262 suite"):
`uv run python scripts/run-test262.py`, plus `cargo test --release` for the crate's
own Rust test suite (including the new integration test if Slice 1 was kept, and
`tests/test262_smoke_oracle.rs`'s random sample).

## 6. Regression risk

- **Every cyclic module evaluation, not just deferred ones**, passes through the
  `inner_module_evaluation` SCC pop loop touched by Fix 1. A cycle with no
  TLA/async members anywhere is unaffected (both old and new code agree: nothing
  has `async_evaluation_order` set, so every popped module becomes `evaluated`).
  The behavior changes only for a cycle where the **root** has no
  TLA/pending-async of its own but a **non-root member** does (or vice versa) —
  today's code incorrectly resolves that whole SCC based on the root's flag alone;
  after the fix each member resolves independently, per spec. This is a strict
  correctness improvement in the same code path already implicated by the issue,
  not scope creep — but it's the part of the fix with the widest blast radius, so
  the `module-code/top-level-await/` and `dynamic-import/` targeted runs above
  exist specifically to catch it.
- `gather_async_transitive_deps` (Fix 4) is shared by both the static `import
  defer` evaluation-list path (`inner_module_evaluation`, `mod.rs:3801-3817`) and
  the dynamic `import.defer()` path (`evaluate_async_transitive_deps`,
  `mod.rs:4206-4218`) — a bug in one shows up in both, and the fix should too.
  `evaluate_async_transitive_deps`'s own `!module.borrow().evaluated` guard at
  `mod.rs:4213` needs no change: once Fix 4 controls what enters `to_eval`, that
  guard is a harmless redundant check against a list that's already correctly
  filtered.
- Shared machinery this leans on, per the standing regression checklist: the
  tree-walker (`execute_module_body_sync`/`execute_async_module` themselves are
  unchanged, only the state-resolution around them); no `property.rs` MOP changes;
  no GC rooting changes; no `ObjectKind` changes; no bytecode fast-path interaction
  (module evaluation is tree-walker only); no Node-compat library-harness
  interaction (module-graph internals aren't exercised by the bundled-library
  smoke tests).
- `test262-pass.txt` is read from `origin/main` per this branch's baseline and is
  **not** rewritten by this plan (no `--update-baseline`).

## 7. Out of scope

- **`ModuleStatus` enum refactor.** The current representation
  (`evaluated: bool`, `is_evaluating: bool`, `cycle_root: Option<ModuleKey>`,
  `async_evaluation_order: Option<u64>`) is boolean-flag soup that is exactly the
  kind of representation that let this bug hide (checking the wrong flag compiles
  fine). A proper `enum ModuleStatus { Linked, Evaluating, EvaluatingAsync,
  Evaluated }` would make illegal states unrepresentable and is worth doing, but
  it's a cross-cutting refactor touching every read/write site enumerated in §3's
  research (`mod.rs` and `eval/modules.rs`), not a bug fix. Flag as a follow-up
  issue; do not bundle into this PR.
- **The `is_evaluating`-vs-`error` interaction at the top of
  `inner_module_evaluation`** (`mod.rs:3778-3789`): `if m.is_evaluating { return
  Ok(index); }` returns without checking `m.error` even though, per spec step 2,
  an `EVALUATING-ASYNC`-or-`EVALUATED` module with a non-empty
  `[[EvaluationError]]` should re-throw. This is currently safe only because jsse
  always sets `evaluated = true` together with `is_evaluating = false` whenever it
  sets `error` (confirmed by the `async_module_execution_rejected` and
  error-propagation call sites in §3's research) — so `is_evaluating == true`
  never coincides with a set error today. Unrelated to this issue; do not touch.
- **`README.md` pass-count update**: deferred to whichever stage actually lands
  the fix and re-runs the full suite (this plan doesn't move code).
