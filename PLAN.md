# Plan: issue #554 — `using` declarations at module top level are never disposed

## 1. Problem restated

A `using` declaration at the top level of a synchronously-evaluated Module (no
top-level `await`, no async dependency) is never disposed when module
evaluation finishes: the resource's `[Symbol.dispose]` is silently dropped
instead of being invoked once the module body completes, whether normally or
via an abrupt (throw) completion. `using` inside functions, blocks, `try`,
`switch`, and `for` bodies is unaffected — this is specific to the
module-evaluation boundary.

## 2. Spec basis

`spec/` is pinned at commit `270a490` (2026-01-21, "Meta: Link to both
single- and multi-page PR previews (#3744)"). Direct verification: `grep -n
"DisposeResources\|UsingDeclaration\|AddDisposableResource" spec/spec.html`
returns **zero matches**. This ecma262 snapshot predates the merge of the
explicit-resource-management proposal into the mainline spec text — it has no
`using` production, no `DisposeResources`, no `AddDisposableResource`, no
`Dispose` abstract operation. Per the repo's own rule 6 ("read-only, use it to
determine syntax and semantics"), `spec/` genuinely does not reach this
feature; the `N/A: no JavaScript behavior change` hatch does not apply either,
since this is squarely a JavaScript semantics fix (a `[Symbol.dispose]` call
that must happen and currently doesn't).

The actual governing text is the **explicit-resource-management proposal**
(`using`/`await using` declarations, `DisposeResources`, `AddDisposableResource`,
`Dispose`), which this codebase already treats as authoritative and has
implemented pervasively — the engine's own code cites its clause numbers
directly:
- `src/interpreter/exec.rs:2349` — `// §14.2.2: DisposeResources for the try block's scope (using declarations)`
- `src/interpreter/exec.rs:2656` — `// §10.4.4.3 Dispose: If method is undefined, result is undefined; else Call(method, V)`
- `src/interpreter/exec.rs:2540` (`add_disposable_resource`) — implements `AddDisposableResource`.

Both test262 files named in the issue declare `esid:
sec-source-text-module-record-execute-module` — the Source Text Module
Record concrete method `ExecuteModule`. In the local spec snapshot this
clause exists at `spec/spec.html:28591-28624` but, consistent with the
grep above, its steps only push a context, evaluate `[[ECMAScriptCode]]`,
and return — no disposal step, because this snapshot predates the proposal's
edit to that clause. The proposal's edit to `ExecuteModule` wraps the result
of evaluating the module body the same way this engine already wraps Block,
Try, Switch, and For bodies: `Set result to Completion(DisposeResources(
module.[[Environment]], result))`, for both the `HasTLA` and non-`HasTLA`
branches, before the completion is returned or the fulfillment promise is
resolved/rejected.

Corroborating evidence already inside the engine: the `HasTLA` (async)
branch is **already correct**. `execute_async_module`
(`src/interpreter/mod.rs:3679-3765`) drives module evaluation through the
same async-function state-machine runtime used for `async function` bodies,
which already calls `self.dispose_resources(&func_env, completion)` at every
terminal completion — see `src/interpreter/eval.rs:8549` (Return),
`eval.rs:8766` (Throw), `eval.rs:9511` (Normal), and the analogous calls in
`src/interpreter/eval/generator_runtime.rs` (e.g. lines 107, 135, 156, 2003,
2005). This was confirmed empirically during planning:

```js
// /tmp/repro_async.mjs — has a top-level await, so module_has_tla() = true
var disposed = false;
var resource = { [Symbol.dispose]() { disposed = true; print("DISPOSED"); } };
using _ = resource;
await Promise.resolve();
print("module body done, disposed=" + disposed);
```
prints `module body done, disposed=false` then `DISPOSED` — correct. The
same script without the `await` (`/tmp/repro_sync.mjs`, forcing the
non-`HasTLA` branch) prints only `module body done, disposed=false` and never
disposes — reproducing the issue exactly. `module_has_tla`
(`src/interpreter/mod.rs:4044-4057`, `stmt_has_tla` at `4059-4123`) already
correctly treats a bare top-level `AwaitUsing` declaration as TLA-triggering
(`mod.rs:4064`), so `await using` at module top level is unaffected by this
bug — only plain `using` in an otherwise-synchronous module is broken.

The one function that drives non-`HasTLA` module bodies,
`execute_module_body_sync` (`src/interpreter/mod.rs:3564-3624`), is the only
place in the module-evaluation machinery with no `dispose_resources` call —
this is the gap ExecuteModule's spec text closes and the fix target.

## 3. Files to touch

- `src/interpreter/mod.rs` — `execute_module_body_sync`
  (`mod.rs:3564-3624`). Add a `dispose_resources(&module_env, completion)`
  call after the module-item loop, mirroring the existing idiom at
  `exec.rs:1033` (Block), `exec.rs:2350` (Try), `exec.rs:2514` (Switch), and
  `exec.rs:1901/2284` (For).
- No parser (`src/parser/`) changes: `using` already parses and binds
  correctly at module top level; `module_has_tla` already classifies plain
  `using` vs. `await using` correctly. This is purely an interpreter
  execution-path gap, not a syntax gap.
- No `docs/adr/` entry: this closes a gap in an already-decided,
  already-implemented architecture (`using`/`DisposeResources` machinery),
  it does not introduce a new architectural decision.
- `test262-extra/` — one new file (name TBD in slice 2, e.g.
  `language-statements-using-module-abrupt-completion-disposal.js`) covering
  abrupt-completion disposal ordering at module scope, which upstream
  test262 does not cover (see §5).

## 4. TDD slices

1. **Red**: run the two test262 files named in the issue —
   `test262/test/language/statements/using/initializer-disposed-at-end-of-module.js`
   and
   `test262/test/language/statements/using/initializer-disposed-at-end-of-imported-module.js`
   — via
   `uv run python scripts/run-test262.py test262/test/language/statements/using/`.
   Both currently fail (timeout/hang on `$DONE()` never being called, since
   disposal — and thus the `assert`/`$DONE()` call inside the disposer —
   never runs).

   **Green**: in `execute_module_body_sync`, after the `for item in
   &program.module_items` loop (`mod.rs:3592-3613`), build the loop's
   completion (`Completion::Throw(e.clone())` if `err` was set,
   `Completion::Normal(JsValue::UNDEFINED)` otherwise) and pass it through
   `self.dispose_resources(&module_env, completion)`, matching on the
   result:
   - `Completion::Throw(e2)` → overwrite `module.borrow_mut().error =
     Some(e2.clone())` and `err = Some(e2)` (a disposer's own throw, or a
     `SuppressedError` combining the body's error with a disposer's error,
     must become *the* module evaluation error — this must overwrite, not
     merge with, whatever `err` already held, since `dispose_resources`
     itself already folds the incoming completion's error into any
     `SuppressedError` it produces).
   - `Completion::Exit(code)` → `self.pending_exit = Some(code); ` and leave
     `err` as `None` (module treated as having evaluated to completion).
     This mirrors the existing, already-reviewed precedent at
     `mod.rs:3755-3762` in `execute_async_module`'s doc comment: "this
     driver returns `()`, so a `__host_exit` from top-level module code is
     recorded in the terminal `pending_exit` sink rather than carried as a
     completion." The async branch already resolves an Exit from a disposer
     this way (it flows through the same `dispose_resources` call inside
     the shared state-machine driver); this keeps both branches consistent.
   - `Completion::Normal(_)` → no-op.

   Place the call before `module.borrow_mut().program_ast = None;` and
   before `self.perf.leave_ast_body()` (so any disposer work still
   attributes to the `<module body>` perf-counter frame per the existing
   convention), and do not hold any `RefCell` borrow of `module` across the
   call (disposers are arbitrary user code that can re-enter the module
   registry — e.g. via dynamic `import()`).

   Re-run the same targeted test262 command; both files must pass. This is
   the single production-code change for this issue — both call sites that
   reach `execute_module_body_sync` (`mod.rs:3876` in
   `inner_module_evaluation`, and `mod.rs:4015` in
   `async_module_execution_fulfilled`'s ancestor-execution loop) are fixed
   by this one edit, since both funnel through the same function.

   Note for the implementer: the *imported-module* test
   (`initializer-disposed-at-end-of-imported-module.js`) is the
   discriminating case — it additionally requires that the disposer's
   mutation (`disposed = true`) be visible to the *importing* module's live
   binding read of `disposed` after the exporting module's
   `execute_module_body_sync` returns. This was spot-checked in isolation
   during planning (a plain post-declaration mutation, e.g. `export let val
   = "before"; val = "after";` read from an importer as `"after"`) and
   works, so no separate live-binding fix is expected — but confirm both
   files pass, not just the entry-module one, before calling this slice
   green.

2. **Red**: upstream test262 only exercises the *normal*-completion
   disposal case (module body completes normally, one resource, no error).
   It does not exercise disposal on an *abrupt* completion of a module body
   (module top level throws after declaring `using` resources), reverse
   (LIFO) disposal order for multiple module-level `using` declarations, or
   `SuppressedError` combination when a disposer throws while the module
   body is already unwinding with an error — all spec-mandated
   `DisposeResources` behaviors, already covered for Block/Try scopes (see
   `test262-extra/DisposableStack-patterns.js` for the LIFO-order
   convention used elsewhere in this repo), but not for the module
   boundary specifically. Add
   `test262-extra/<name>-module-abrupt-completion-disposal.js`: a
   `flags: [module, async]` entry importing a `_FIXTURE.js` module that
   declares two `using` resources (each appending to a shared log via
   `globalThis`) and then throws; the entry's `import(...)` rejects, and the
   test asserts (a) the rejection reason is the expected `SuppressedError`/
   plain error, and (b) the log shows both resources disposed in reverse
   declaration order despite the abrupt completion. This file does not
   exist yet, so it is red by construction (no such assertions currently
   run against this path).

   **Green**: expected to pass with the same slice-1 fix and no further
   production change, since `dispose_resources` already implements
   LIFO-order disposal and `SuppressedError` combination generically
   (`exec.rs:2648` `stack.reverse()`, `exec.rs:2680-2682`
   `wrap_suppressed_error`) — slice 2 exists to *lock in* module-scope
   coverage of behavior that was already implemented once, not to add new
   engine logic. If it does not pass as-is, that is new information
   requiring a follow-up fix inside `execute_module_body_sync`, not a
   pre-known step.

## 5. Test surface

- Targeted: `uv run python scripts/run-test262.py
  test262/test/language/statements/using/` (both the two issue files and
  the existing `syntax/using.js` /
  `syntax/using-allowed-at-top-level-of-module.js`, which must keep
  passing).
- Targeted: `uv run python scripts/run-test262.py
  test262/test/staging/explicit-resource-management/` — includes
  `await-using-in-top-level-module.js`, the already-correct TLA sibling
  case; must keep passing (regression guard on the branch this change does
  *not* touch).
- Broad regression guard (shared code path): `uv run python
  scripts/run-test262.py test262/test/language/module-code/` — every
  synchronously-evaluated module test in the suite exercises
  `execute_module_body_sync`.
- Full suite: `uv run python scripts/run-test262.py` (no `--update-baseline`
  — that is a `main`-branch operation, not part of this plan). Compare
  against the `origin/main:test262-pass.txt` baseline for regressions and
  new passes.
- New spec-correct-but-uncovered behavior: `test262-extra/` file from TDD
  slice 2, run via `uv run python scripts/run-test262.py test262-extra/`.
- `cargo test --release` (per project convention; note the `.rs`-file
  post-edit hook already runs `cargo build`/`clippy -D warnings` on save).

## 6. Regression risk

- **Blast radius**: `execute_module_body_sync` runs for every
  synchronously-evaluated module in the entire test262 corpus (all of
  `language/module-code/`, plus every test that uses a module as an import
  fixture). The fix must be a true no-op for the overwhelming majority of
  modules that declare no top-level `using`. This is structurally
  guaranteed by `dispose_resources` itself: when `env.dispose_stack` is
  `None` or empty, it returns the input completion unchanged after only a
  `take()` and an `is_none()`/`is_empty()` check (`exec.rs:2640-2646`) — no
  observable behavior change and negligible cost for `using`-free modules.
- **`module.error` bookkeeping**: `inner_module_evaluation`'s cycle-root
  marking (`mod.rs:3879-3896`) and `async_module_execution_rejected`'s
  parent-propagation (`mod.rs:3963-3984`) both read `module.error`. A
  dispose-time throw must overwrite `module.error`
  exactly like the existing statement-throw branch already does
  (`mod.rs:3596-3600`), or a dependent module / re-import will observe a
  stale or missing error.
- **`pending_exit` sink**: must follow the existing, already-reviewed
  precedent at `mod.rs:3755-3762` for `Completion::Exit` from a disposer, to
  avoid silently swallowing a `__host_exit` call made from a module-level
  disposer (issue #242's sink convention).
- **Tree-walker only, no bytecode/GC concern**: module items reach
  `exec_statement` directly, bypassing `dispatch_body`
  (`mod.rs:3580-3583` comment) — the bytecode fast path is not implicated.
  `dispose_resources` calls into `self.call_function`, the same primitive
  already used identically by the Block/Try/Switch/For call sites, so no
  new GC-rooting surface is introduced.
- **Not touching**: `test262-pass.txt` baseline rewriting (main-branch-only
  operation, explicitly out of scope per task constraints).

## 7. Out of scope

- The pre-existing, unrelated gap in `execute_module_body_sync`'s
  module-item loop (`mod.rs:3592-3613`): it only inspects
  `Completion::Throw` from a statement and silently continues past a
  `Completion::Exit` produced directly by a module-top-level statement
  (not by disposal). This is a real gap but is not what issue #554 reports,
  and fixing it is a separate, larger change to the loop's control flow
  that would benefit from its own issue and test coverage rather than
  being bundled into this disposal fix.
- Whether plain (non-module) Script top-level `using` is parsed/rejected
  correctly. Not implicated by this issue and not touched.
- No refactor of the several near-duplicated module-setup blocks in
  `mod.rs` (e.g. `run_module` at `~2289`, and similar module-environment
  construction blocks around `~1142`, `~3015`, `~3241`, `~3421`) — a
  tempting cleanup target given how much of this investigation walked
  through them, but unrelated to this bug and out of scope for a bug-fix
  PR ("many small changes beat one large change").
- No change to `test262-pass.txt` (read-only in this workflow; rolled
  forward only on `main`).
