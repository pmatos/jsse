# Plan: issue #596 — Module DFS keeps evaluating sibling/parent modules after a child's `__host_exit`

## 0. Status (workspace reused across pipeline runs)

All three TDD slices below are already implemented on this branch, verified commit-by-commit
against §3 to match exactly:

- Slice 1 (sibling + parent bail in `inner_module_evaluation`) → `9707f4b`
- Slice 2 (async/TLA body-execution branch coverage) → `1e7ae31`
- Slice 3 (`evaluate_async_transitive_deps` bail + `ImportDefer` sink conversion) → `2dd7896`

Line-number references in §3 were taken before these commits landed and are now off by a few
lines; use the commit diffs (`git show 9707f4b 2dd7896`) as the authoritative location, not the
numbers below. The branch has never been pushed and no PR exists yet (confirmed via
`gh pr list --head <branch> --state all` → `[]`; see `EVIDENCE.md` and the issue comment at
2026-09-04T11:41:39Z) — a prior "simplify" pipeline stage ran against this workspace expecting an
open PR and correctly reported that blocker. The next stage should verify
(`cargo test --release host_exit`, `./scripts/lint.sh`), skip re-implementing §3/§4, `git rm
PLAN.md`, push the branch, and open the PR.

## 1. Problem restated

`__host_exit` (jsse's Node-`process.exit`-compatible host hook, opt-in via `enable_node_host()`)
propagates structurally as `Completion::Exit` up to the boundary that owns each module's body
(`execute_module_body_sync` for the synchronous path, `execute_async_module`/`async_function_resume`
for the top-level-await path). Both boundaries correctly latch the code into the terminal
`self.pending_exit` sink and return a plain `Ok(())` / `()` — by design, since neither has a
completion channel back to its caller. The bug is one level up: `inner_module_evaluation`, the
Tarjan-SCC module-graph DFS (spec `InnerModuleEvaluation`), calls those boundaries and then
keeps walking — it never reads `self.pending_exit` between DFS steps. So once a module deep in
the graph exits, the DFS still evaluates that module's remaining sibling dependencies *and* the
parent module's own body, which can call `__host_exit` again with a different code and silently
overwrn the first — breaking the first-exit-wins invariant asserted by
`host_exit_is_uncatchable_and_records_code` and friends in `src/interpreter/tests.rs`. A second,
related boundary (`import.defer()`'s dynamic-import handler in `eval.rs`) has the same shape: it
drives eager evaluation of a deferred import's async transitive deps and then unconditionally
resolves a promise and lets the calling module body keep running, without checking whether that
eager evaluation just latched an exit.

## 2. Spec basis

- `spec/spec.html#sec-innermoduleevaluation` — **InnerModuleEvaluation** (the abstract operation
  `inner_module_evaluation` implements: DFS-index/ancestor-index bookkeeping, the loop over
  `[[RequestedModules]]` that recurses per dependency, and the SCC-closing loop that transitions a
  strongly-connected component to `evaluating-async`/`evaluated`). This clause defines the
  traversal order and bookkeeping that must be preserved unchanged for every program that never
  calls `__host_exit`.
- `__host_exit` itself is **host-defined**, not part of ECMA-262 — it exists purely as a
  jsse-internal Node-compat test/debug hook (`src/interpreter/builtins/node_host.rs`,
  `enable_node_host()`), the same status the codebase already gives it in the #242/#554/#583
  commit trail. No test262 test can observe it (confirmed: no `__host_exit` references anywhere
  under `test262/`), and the test262 runner never calls `enable_node_host()`.
- Net effect: this fix changes **zero** observable behavior for any spec-conformant module graph.
  It only changes behavior on the non-standard path that is reachable exclusively through
  `__host_exit`, by making `InnerModuleEvaluation`'s Rust implementation stop advancing once the
  host has unconditionally terminated the program — consistent with how every other completion
  boundary in this engine (`drain_microtasks`, `run_due_timers`, the `for-of` iterator-close path
  at `eval.rs:9501`, the generic async-function-call path at `eval.rs:8358`) already treats
  `pending_exit` as a terminal, uncatchable signal that supersedes further JS execution. This is
  not a new JS language feature; it is closing a gap in an existing host-exit invariant, so the
  `N/A: no JavaScript behavior change` hatch does not apply cleanly either — the InnerModuleEvaluation
  clause above is the concrete anchor for what the change must *not* disturb.

## 3. Files to touch

- `src/interpreter/mod.rs`
  - `inner_module_evaluation` (~line 3839–3969): add early-bail checks (below).
  - `evaluate_async_transitive_deps` (~line 4282–4295): add an early-bail check in its loop.
- `src/interpreter/eval.rs`
  - `Expression::ImportDefer` arm of `eval_expr` (~line 1246–1289): convert a latched
    `pending_exit` back into a structural `Completion::Exit` after eager transitive-dep
    evaluation, matching the existing idiom at `eval.rs:9501-9503` and `eval.rs:9893-9895`.
- `src/interpreter/tests.rs`: new tests only, no production changes.
- No `docs/adr/` entry — this is a bug fix restoring an already-documented invariant
  (first-exit-wins, "nothing runs after `__host_exit`"), not a new architectural decision. No
  `CONTEXT.md` changes — no new vocabulary.

### Exact production edits

In `inner_module_evaluation`'s dependency loop (currently `for dep_canon in
evaluation_list.into_iter() { idx = self.inner_module_evaluation(&dep_canon, stack, idx)?; ...
}`, ~line 3892-3936): immediately after the recursive call, add

```rust
idx = self.inner_module_evaluation(&dep_canon, stack, idx)?;
if self.pending_exit.is_some() {
    return Ok(idx);
}
```

This stops a *sibling* dependency (and the transitively-owning parent) from starting once any
earlier dependency has exited — it is the direct fix for the "sibling" half of the issue title.

After the module's own body executes (currently ~line 3938-3949, the
`if has_tla || pending > 0 { ... } else { self.execute_module_body_sync(&canon)? }` block),
before the SCC-closing loop (~line 3951-3967):

```rust
if self.pending_exit.is_some() {
    return Ok(idx);
}
```

This covers both the sync path (`execute_module_body_sync`, which already returns `Ok(())` after
latching the sink at line 3681) and the async/TLA path (`execute_async_module`, which latches the
sink at line 3833 when `__host_exit` fires in a TLA module's pre-`await` prefix —
`async_function_resume`'s own doc comment, ~line 8364-8368, and the precedent at `eval.rs:8351-8360`
confirm it returns `Completion::Exit` synchronously in that case rather than deferring to a
microtask). It also directly fixes the "parent" half of the issue title: a parent module's own
top-level statements must not run once a dependency has exited.

Skipping the SCC-closing loop (the `while let Some(popped) = stack.pop() { ... }` block) on
bail-out is deliberate, not an oversight: nothing reads `is_evaluating`/`evaluated`/`cycle_root`
after `pending_exit` is set (every driver — `main.rs`, `drain_microtasks`, `run_due_timers` — treats
it as terminal and stops), and `stack` is a plain local `Vec` per entry-point call
(`run_module`'s `let mut stack = vec![]` at line 2496, or `evaluate_async_transitive_deps`'s fresh
`let mut stack = vec![]` per iteration at line 4291) that is simply dropped, not the module
registry itself.

No sink guard (`if self.pending_exit.is_none() { self.pending_exit = Some(code); }`) is planned in
addition to the bail-early checks. The DFS is strictly single-threaded/sequential, so once the
first exit's bail-early check fires, no code path remains that could reach a second
`self.pending_exit = Some(...)` write — the guard would be dead defense-in-depth, not a
requirement. (Considered per the issue's "guard the sink and/or bail early" phrasing; rejected as
redundant once bail-early is applied at every call site below.)

In `evaluate_async_transitive_deps`'s loop (~line 4287-4294):

```rust
for path in to_eval {
    if let Some(module) = self.module_registry_get(&path)
        && !module.borrow().evaluated
    {
        let mut stack = vec![];
        let _ = self.inner_module_evaluation(&path, &mut stack, 0);
        if self.pending_exit.is_some() {
            return;
        }
    }
}
```

This is the *other* DFS-driving loop the issue names (deferred-import eager evaluation of async
transitive deps, triggered by `import.defer()`/`import defer * as ns from`): without this, one
transitive dep exiting does not stop the next one in `to_eval` from being evaluated.

In `eval.rs`'s `Expression::ImportDefer` arm, between the `self.drain_microtasks();` call and
building the resolved promise (~line 1282-1285):

```rust
self.evaluate_async_transitive_deps(&resolved_canon);
self.drain_microtasks();
if let Some(code) = self.pending_exit {
    return Completion::Exit(code);
}
let ns = self.create_deferred_module_namespace(&module);
self.create_resolved_promise(ns)
```

Without this, the `evaluate_async_transitive_deps` loop fix alone is invisible from the calling
module: `eval_expr` would still return `Completion::Normal` (a resolved promise) regardless of a
latched exit, and the module body containing the `import.defer()` call would keep running its own
trailing statements. This mirrors the identical pattern already used for the `for-of`
iterator-close boundary (`eval.rs:9501-9503`) and the generic `await` boundary
(`eval.rs:9893-9895`) — "if a drained job requested `__host_exit`, propagate the exit out of this
boundary uncatchably instead of yielding a value."

## 4. TDD slices

1. **Sibling + parent bail in the main DFS loop (sync body).**
   Red: add `host_exit_in_dependency_stops_sibling_import_and_parent_body` to
   `src/interpreter/tests.rs` (same helpers as `host_exit_from_module_top_level_using_skips_disposer_and_records_code`:
   `temp_case_dir`, `write_case_file`, `parse_module_program`, `interp.enable_node_host()`,
   `interp.run_with_path`). Layout: `main.mjs` does
   `import "./child.mjs"; import "./sibling.mjs"; globalThis.parentRan = "yes";`; `child.mjs` sets
   `globalThis.childRan = "yes"; __host_exit(3);`; `sibling.mjs` sets
   `globalThis.siblingRan = "yes";`. Assert `interp.pending_exit == Some(3)`,
   `global_string(&interp, "childRan") == "yes"`, and both
   `interp.get_global_var_ref("siblingRan").is_none()` and
   `interp.get_global_var_ref("parentRan").is_none()` (mirrors the existing `.is_none()` assertion
   pattern at `tests.rs:961/1019/1061`). This fails today because `sibling.mjs` and `main.mjs`'s
   own body both run.
   Green: the two `if self.pending_exit.is_some() { return Ok(idx); }` checks in
   `inner_module_evaluation` described above.

2. **Same bail on the async/TLA body-execution branch.**
   Red: add `host_exit_in_top_level_await_dependency_stops_sibling_and_parent`. `child.mjs`
   contains a top-level `await` (so `module_has_tla` is true and it routes through
   `execute_async_module`) but calls `globalThis.childRan = "yes"; __host_exit(4);` *before* that
   await, so the exit is synchronous within `async_function_resume`'s initial resume. Same
   `main.mjs`/`sibling.mjs` shape as slice 1. This exercises the `execute_async_module` branch of
   the same `if has_tla || pending > 0 { ... } else { ... }` block; it should already be green
   after slice 1's fix (both branches share the single post-block bail check) — write it as its
   own red/green pair anyway since it is a materially different code path (confirms the fix isn't
   accidentally coupled to `execute_module_body_sync`'s `Ok`/`Err` shape).

3. **Sibling bail in `evaluate_async_transitive_deps`'s loop, plus the `import.defer()` sink
   conversion.**
   Red: add `host_exit_in_deferred_transitive_dep_stops_sibling_transitive_dep_and_caller`.
   `main.mjs`: `import.defer("./a.mjs"); globalThis.mainAfter = "yes";` (the trailing statement is
   the regression check for the `eval.rs` fix — without it, a green `evaluate_async_transitive_deps`
   loop alone would still let this line run). `a.mjs`: `import "./b.mjs"; import "./c.mjs";` (both
   non-deferred; `load_module_no_eval`'s "Pre-load pass: load sub-dependencies", ~`mod.rs:3374-3385`,
   registers both without evaluating them, so `gather_async_transitive_deps` can find them).
   `b.mjs`: has a top-level `await` for `has_tla`, but calls
   `globalThis.bRan = "yes"; __host_exit(11);` before it (same synchronous-exit shape as slice 2).
   `c.mjs`: also has a top-level `await` (so it too is an "async transitive dep" leaf per
   `gather_async_transitive_deps`'s `has_tla` check) and sets `globalThis.cRan = "yes";`. Drive it
   with `interp.run_with_path` on `main.mjs` (the `import.defer()` call is a statement-position
   dynamic import, so no extra `.then()`/drain plumbing is needed beyond what `run_with_path`
   already does). Assert `interp.pending_exit == Some(11)`,
   `global_string(&interp, "bRan") == "yes"`, and `cRan`/`mainAfter` both
   `interp.get_global_var_ref(..).is_none()`.
   Green: the `evaluate_async_transitive_deps` loop check and the `eval.rs` `ImportDefer` arm
   check described above.

Run `cargo test --release host_exit` (or the full `cargo test --release`) after each slice; do not
move on to the next slice until the previous one is green.

## 5. Test surface

- No `test262/` directory exercises this: `__host_exit` is not observable via any spec-conformant
  program (confirmed via repo-wide grep — zero hits under `test262/`), so there is no targeted
  test262 subdirectory to run for this change specifically.
- No new `test262-extra/` test either, for the same reason — that directory is for spec-correct
  behavior test262 doesn't cover; this behavior is host-specific by construction, not spec-correct
  behavior at all.
- All three new tests belong in `src/interpreter/tests.rs`, alongside every existing `__host_exit`
  test (`host_exit_is_uncatchable_and_records_code`,
  `host_exit_from_module_top_level_using_skips_disposer_and_records_code`,
  `host_exit_from_module_export_initializer_stops_body`, `host_exit_from_async_reaction_stops_drain`,
  `host_exit_skips_iterator_return_cleanup`).
- Gate: `cargo test --release` (per CLAUDE.md; module-graph tests use real temp-dir files, so
  `--release` isn't required for correctness but matches the project's default build mode).
- Also run the full test262 suite once (`uv run python scripts/run-test262.py`) to confirm zero
  baseline movement, per CLAUDE.md's "after any implementation work, run the full test262 suite" —
  expected to be a no-op since `enable_node_host()` is never called by the runner, so every new
  `self.pending_exit.is_some()` check is reading a field that stays `None` for the entire run.
- Also run `./scripts/lint.sh` (clippy gate) since production files change.

## 6. Regression risk

- **`test262-pass.txt` baseline:** expected to be completely unaffected. `pending_exit` is `None`
  unless `enable_node_host()` ran (`src/main.rs:135-142`, gated behind the CLI's `--node` flag),
  and `scripts/run-test262.py` never sets that flag. Every check added by this fix is therefore a
  cheap `Option::is_some()` read against a value that is always `None` during a test262 run — an
  early-return branch that is never taken, not a change to any evaluated-in-test262 code path.
- **Shared machinery leaned on:** `inner_module_evaluation` is the sole DFS driver for both static
  module linking (`run_module`) and dynamic `import()`/`import.defer()`; `evaluate_async_transitive_deps`
  is the sole deferred-import eager-evaluation driver. Both are exercised heavily by every existing
  module test in `src/interpreter/tests.rs` (cycles, star re-exports, dynamic import, deferred
  namespaces) — those are the tests most likely to catch an accidental behavior change on the
  non-exit path, since the new checks are `if`-guarded on a field that's `None` there too.
- **Not touched:** the tree-walker hot paths (`eval_expr`/`exec_statement` outside the two edited
  arms), the property MOP in `property.rs`, GC rooting/`gc_safepoint()`, the `ObjectKind` matches,
  and the bytecode fast path — none of them run module-graph DFS code.
- **Node-compat library harnesses:** none of the pinned libraries (`decimal.js`, `acorn`, `zod`,
  `luxon`, etc., per CLAUDE.md's "Library Tests" section) call `__host_exit`, and the harness
  doesn't enable the node host floor for that reason either — no expected impact, not re-run as
  part of this fix.

## 7. Out of scope

- Any change to `async_module_execution_fulfilled`/`async_module_execution_rejected` or the
  `resolve_fn`/`reject_fn` continuations — the issue's own text confirms "the async module path
  (3782-3787) already latches correctly," and those continuations run as ordinary microtask-queue
  jobs, already protected by `drain_microtasks`'s per-job `pending_exit` check (`mod.rs:5441-5443`).
  No evidence they share this bug.
- Adding the redundant sink guard (`if self.pending_exit.is_none() { ... }`) at the two latch
  sites (`mod.rs:3681`, `mod.rs:3833`) — considered and rejected in §3 as dead code once bail-early
  is applied everywhere the DFS can advance.
- Any refactor of `inner_module_evaluation`'s Tarjan bookkeeping, or consolidating the three
  `inner_module_evaluation` call sites (`run_module`, the dependency loop, `evaluate_async_transitive_deps`)
  behind a shared helper — the fix is three small, independent `if`-checks; a shared abstraction
  for three call sites with different loop shapes (one non-looping, two looping over different
  collections) would be premature.
- Formatting/cleanup of surrounding code not touched by this fix.
