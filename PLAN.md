# Plan: issue #596 — module DFS keeps evaluating sibling/parent modules after a child's `__host_exit`

## Status note (read first)

This workspace was reused from an earlier attempt. Inspection of `git log` shows the fix and
its tests are **already implemented and committed** on this branch:

- `a0b3cee` fix(module): stop sibling/parent DFS steps after a dependency's `__host_exit`
- `c970fec` test(module): cover `__host_exit` DFS bail on the top-level-await body branch
- `c8fb388` fix(module): stop deferred-import transitive-dep DFS after `__host_exit`

Verified in this session (2026-09-05), on top of the current HEAD:

- `cargo build --release` — clean.
- `cargo test --release` — all 646+ unit/integration tests pass, including the three
  targeted regression tests below.
- `./scripts/lint.sh` — rustfmt + clippy (default and `perf-counters`) all clean.

No PR exists yet for this branch (`gh pr list --head <branch>` returns empty). The remaining
work is verification and PR creation, not new code. This plan documents what was already done
so the implementation stage can confirm it, add nothing further unless a gap is found, and open
the PR.

## 1. Problem restated

`execute_module_body_sync` (and `execute_async_module`) latch `self.pending_exit` when a module
body calls the host hook `__host_exit`, but the module-graph depth-first traversal driven by
`inner_module_evaluation` did not check `pending_exit` between steps. Concretely: after a
dependency's recursive `inner_module_evaluation` call returned (having latched `pending_exit`
internally), the caller kept iterating over remaining sibling `RequestedModules` and then went
on to execute its own module body — so a `child.mjs` calling `__host_exit(7)` would not stop a
sibling import or the importing parent's body from running, and a later `__host_exit(3)` in that
parent could overwrite the first exit code, violating the first-exit-wins invariant the engine
otherwise guarantees for `__host_exit`. A second, structurally identical DFS driver,
`evaluate_async_transitive_deps` (used by `import.defer()`'s eager evaluation of a deferred
import's async transitive dependencies), had the same bug: it kept evaluating remaining
transitive deps after an earlier one latched `pending_exit`.

## 2. Spec basis

`__host_exit` is a JSSE-internal host hook (not an ECMA-262 builtin) that models a host
terminating the running agent; `pending_exit` is deliberately implemented as a side sink outside
the normal `Completion` record plumbing (see the comment at `src/interpreter/mod.rs:2264`),
because a real host exit is not a throw/return/abrupt completion representable in-language.

The DFS structure it must not violate, however, is exactly the one specified in
**ECMA-262 §16.2.1.5.3.1 `InnerModuleEvaluation ( module, stack, index )`**
(spec/spec.html, `id="sec-innermoduleevaluation"`):

- Step 10 iterates `module.[[RequestedModules]]` and at step 10.a sets
  `index` to **`?` InnerModuleEvaluation(requiredModule, stack, index)`** — the `?` prefix is
  `ReturnIfAbrupt`: any abrupt outcome from a nested call must immediately abort the loop over
  the remaining requested modules, not just be recorded and continue.
- Step 9 (the non-cyclic module case) and step 12 similarly gate `module.ExecuteModule()` /
  further bookkeeping on the recursive evaluation of dependencies having completed without an
  abrupt outcome that already terminated the algorithm.

A host-initiated termination is a stronger signal than a normal abrupt completion (spec engines
are explicitly permitted to stop running ECMAScript code at all when the host decides to exit —
this is the general "the host may terminate the agent" premise that underlies hooks like
`HostEnsureCanCompileStrings` and the module job pipeline's coordination with host callbacks).
`pending_exit` must therefore satisfy at least the same "stop the DFS immediately, do not touch
sibling requested modules or the current module's own body" contract that `?` encodes for
in-language abrupt completions in §16.2.1.5.3.1, even though it is carried out-of-band. This is
the spec grounding the issue calls for: not a new piece of JavaScript syntax or semantics, but a
correctness requirement on the DFS shape that #16.2.1.5.3.1 itself specifies, which the
host-exit side channel must not bypass.

`import.defer()`'s eager evaluation of transitive dependencies is host/implementation-defined
scheduling around the "defer" proposal semantics, not itself a numbered spec algorithm in the
checked-in `spec/spec.html` (Source Phase Imports lives in a separate proposal repo, not the
mainline ECMA-262 submodule here) — but `evaluate_async_transitive_deps` is JSSE's own DFS over
the same module graph, walked for the same reason, so the identical first-exit-wins contract
applies by construction, not by a separate spec clause.

## 3. Files touched (already touched, verified)

- `src/interpreter/mod.rs`
  - `inner_module_evaluation` (~line 3897–3963 on current HEAD): bail with `Ok(idx)` immediately
    after each dependency's recursive `inner_module_evaluation` call if `pending_exit` is now
    `Some`, and again immediately after the module's own body executes
    (`execute_module_body_sync` / `execute_async_module` return), before touching
    `dfs_index`/`dfs_ancestor_index` bookkeeping.
  - `evaluate_async_transitive_deps` (~line 4303–4306): bail out of its loop over transitive
    deps as soon as `pending_exit` is set, instead of continuing to the next dependency.
- `src/interpreter/eval.rs`
  - The `ImportDefer` eval arm (~line 1281–1287): after `evaluate_async_transitive_deps` +
    `drain_microtasks()`, convert a latched `pending_exit` into a structural
    `Completion::Exit(code)` so the *calling* module's own trailing statements stop too —
    matching the existing for-of/`await` `__host_exit` propagation idiom used elsewhere in the
    interpreter.
- `src/interpreter/tests.rs`
  - `host_exit_in_dependency_stops_sibling_import_and_parent_body` (~line 3729): sync module
    body DFS bail (the literal repro shape from the issue: `child.mjs` exits, `parent.mjs` must
    not run its own body or a later sibling import).
  - `host_exit_in_top_level_await_dependency_stops_sibling_and_parent` (~line 3773): same
    invariant on the `execute_async_module` / top-level-await body branch.
  - `host_exit_in_deferred_transitive_dep_stops_sibling_transitive_dep_and_caller` (~line 3818):
    the second DFS driver (`import.defer()`'s transitive-dep eager evaluation), confirming a
    `Completion::Exit` reaches the calling module's own trailing statements.

No other files require changes. No `docs/adr/` entry is warranted — this is a bug fix to an
existing internal invariant, not a new architectural decision.

## 4. TDD slices (as executed; recorded for the implementation stage to confirm, not repeat)

1. **Red**: a test constructing `parent.mjs` importing `child.mjs` where `child.mjs` calls
   `__host_exit(7)` before a later sibling import module runs; assert the sibling and the
   parent's own body never execute and the recorded exit code is `7`, not whatever a later
   statement would have produced. This is `host_exit_in_dependency_stops_sibling_import_and_parent_body`.
   Before the fix, this test fails because the sibling/parent body still ran.
   **Green**: add the `if self.pending_exit.is_some() { return Ok(idx); }` guards in
   `inner_module_evaluation` after the dependency-evaluation loop step and after the module body
   execution step.
2. **Red**: the same shape but routed through the top-level-await/async module execution path
   (`execute_async_module`) rather than the plain sync body path, to confirm the same guard in
   `inner_module_evaluation` covers both callers of "evaluate this module's body" since the
   check lives after both branches converge. This is
   `host_exit_in_top_level_await_dependency_stops_sibling_and_parent`.
   **Green**: no additional production change needed beyond slice 1 — the guard's placement
   after the `if is_async { execute_async_module } else { execute_module_body_sync }` branch
   already covers it; the test documents that it does.
3. **Red**: `import.defer()` eagerly evaluating a deferred import's async transitive
   dependencies, where one transitive dep calls `__host_exit`; assert a later sibling transitive
   dep does not evaluate and the *calling* module's statements after the `import.defer()`
   expression do not run. This is
   `host_exit_in_deferred_transitive_dep_stops_sibling_transitive_dep_and_caller`.
   **Green**: add the `pending_exit` guard inside `evaluate_async_transitive_deps`'s loop, and
   convert a latched `pending_exit` into `Completion::Exit(code)` in the `ImportDefer` eval arm
   so the calling module's own trailing statements stop, reusing the existing
   `Completion::Exit` propagation idiom already used for `for`-`of`/`await` `__host_exit` cases.

All three slices are green on current HEAD (verified via
`cargo test --release host_exit_in_ -- --test-threads=4`, 3 passed).

## 5. Test surface

- No `test262/test/...` directory exercises this: `__host_exit` is a JSSE-internal test/CLI
  host hook, not an ECMA-262 builtin, so test262 has no coverage surface for it and none should
  be added there.
- The regression coverage lives in `src/interpreter/tests.rs` (unit tests, run via
  `cargo test --release`), which is the correct location per the project's own convention for
  "exact host-compatibility diagnostics" — `__host_exit` is exactly that. `test262-extra/` is
  not appropriate here since there is no ECMA-262 clause under test.
- Full gate for this change: `cargo test --release` (already green, 646+ tests) and
  `./scripts/lint.sh` (already green). A targeted test262 run is not expected to move and is
  low-value here, but the implementation stage should still run the standard
  `uv run python scripts/run-test262.py` gate as normal project hygiene before opening the PR,
  since `src/interpreter/mod.rs` and `eval.rs` are hot paths shared by all module/import
  evaluation.

## 6. Regression risk

- `inner_module_evaluation` is the sole DFS driver for **all** module linking/evaluation
  (static imports, dynamic `import()`, top-level modules, cycles). The new early-return guards
  only trigger when `pending_exit.is_some()`, which is `None` for every test262 test and every
  normal program that never calls `__host_exit` — so the guards are no-ops on the hot path and
  should not move `test262-pass.txt`.
- `evaluate_async_transitive_deps` is only reachable via `import.defer()` (Source Phase
  Imports), a narrow feature surface; the same "only active when `pending_exit` is set" argument
  applies.
- The `ImportDefer` eval arm now returns `Completion::Exit(code)` in the `pending_exit` case
  instead of always producing a resolved promise — this changes control flow only on the
  already-exiting path, which by definition means the process is tearing down, so no downstream
  code should observe the difference.
- No interaction with GC rooting, the `ObjectKind` matches, the bytecode fast path, or the
  Node-compat library harnesses: this is pure control-flow in the tree-walker's module-evaluation
  driver, not a new object shape or opcode.

## 7. Out of scope

- No refactor of `inner_module_evaluation`'s broader structure (e.g. extracting the DFS into a
  named helper, deduplicating the two guard sites) — the issue asks for a correctness fix, not a
  cleanup, and the two guards are small and clear as inline checks.
- No attempt to make `pending_exit` a first-class `Completion` variant threaded through every
  return type in the module pipeline — the issue text explicitly frames that as the
  larger-scope alternative that was deferred from #583; the guard-check approach implemented
  here is the minimal fix consistent with the existing `pending_exit` sink design.
- No changes to `test262-pass.txt` (read-only from `origin/main` per project convention; not a
  `main`-branch operation available here).
- No changes to `spec/` or `test262/` submodules.
