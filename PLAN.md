# Plan: opt-in deterministic step budget (#1173)

## Problem restated

The interpreter has no way to stop a runaway script, so fuzz targets must run `jsse` as a subprocess with a wall-clock kill (ADR 0004). Add an opt-in **Step Budget**: a counter ticked at every existing GC safepoint (statement boundaries and loop back-edges) in both the bytecode VM (`Op::Safepoint`, negative-offset `Op::Jump`) and the tree-walker (`gc_safepoint()` sites). When the budget is exhausted the run ends with an **uncatchable** outcome, reusing the existing `Completion::Exit` / `pending_exit` machinery. With no budget set the safepoint path must stay one predictable flag test. The budget is exposed through a public library API (for in-process fuzz targets) and a `--step-budget N` CLI flag.

## Spec basis

N/A: no JavaScript behavior change. The budget is a host resource limit. ECMAScript has no step-limit concept (it is the same class of host limit as the recursion `RangeError` guard and the `RLIMIT_AS` cap). With no budget set, every spec-observable behavior is byte-identical. With a budget set, exhaustion terminates the whole run uncatchably, which is the host "terminate execution" prerogative. No clause is relied on.

## Assumptions

- **Exhaustion is fatal and uncatchable, not a catchable throw** (best guess, recorded in the ADR). A catchable throw is unsound for the stated purpose: `for(;;){ try { for(;;); } catch(e){} }` would swallow the exhaustion forever, and a `finally` could run unbounded work. `Completion::Exit` is already structurally uncatchable (skips `catch`, `finally`, disposers, iterator `return()` and Promise reactions; `exec_try` and `drain_microtasks` already honor it). Alternative considered and rejected: a catchable `RangeError` that re-throws at every later safepoint. It still lets `finally` blocks run user code and complicates every `catch` site.
- **One step = one counted safepoint.** The unit is "safepoints reached", so the count is deterministic *per execution mode*. It is not equal between bytecode and `--no-bytecode`: the compiler drops the safepoint of a statement that compiles to nothing (`;`, `var a;`), and a hand-off node runs its statements on the tree-walker, which ticks them itself. Both are deterministic across reruns. The count is deterministic only for scripts that do not read clocks or entropy (`Date.now`, `performance.now`, unseeded `Math.random`) and do not observe GC (WeakRef/FinalizationRegistry callbacks differ under `JSSE_GC_STRESS`). For other scripts GC stress and the randomised atom interner do not change the count, because safepoint locations are fixed.
- **Event-loop ticks (added after review).** Expression-bodied callbacks have no statement or back-edge, so `const f = () => Promise.resolve().then(f); f()` and `setInterval(() => 0, 0)` would spin the event loop forever. The budget therefore also ticks once per microtask job (`drain_microtasks`, mod.rs:5406) and once per timer callback (`run_due_timers`, mod.rs:5516). Off the hot path (per job, not per op). On exhaustion latch `pending_exit` and return from the loop.
- **Known residual hang (documented in the ADR):** native iterator-protocol loops whose user `next` is an expression arrow over a non-growing collection, e.g. `new Set({[Symbol.iterator]: () => ({next: () => ({value: 1, done: false})})})` (same for `new Map`, `Array.from`, spread), never reach a safepoint. The fuzz-target follow-up needs a watchdog or function-entry ticks.
- **Not counted:** `gc_safepoint()` calls in `run_program`/`run_program_with_path` (mod.rs:2386, 2429) and `$262.gc` (mod.rs:963). They are GC-only calls, not statement or back-edge safepoints. `run_program`'s extra safepoint is one constant per program.
- **Unbounded work that never reaches a safepoint is out of scope.** Examples: expression-bodied-arrow exponential recursion `n => n ? f(n-1)+f(n-1) : 1` (the depth guard still applies), catastrophic regex backtracking, huge native loops (`Array(2**32-1).fill`, `"x".repeat(2**28)`). The ADR states this. Counting function entries or native-loop iterations is a follow-up.
- **Atomics agent threads** build their own `Interpreter`; they do not inherit the budget. The ADR notes it.
- **Exit code 124** (timeout convention), exposed as `STEP_BUDGET_EXIT_CODE`. With `--node`, `__host_exit(124)` shares the code, so callers distinguish via `step_budget_exhausted()` and the CLI stderr message, not the code alone.
- **No new fuzz target in this PR.** The AC only requires the API to be reachable from the library the fuzz crate links. A target is a follow-up under #1168.
- **No test262-extra test.** A test262-extra test cannot enable a budget (the runner passes only `--no-bytecode`/`--can-block`), and the budget has no ECMAScript-observable effect when unset. The resource-limit checks belong in `tests/` per AGENTS.md ("engine resource-limit or stress checks remain in `tests/`").

## Files to touch

| File | Role | Lines of interest |
|------|------|-------------------|
| `src/interpreter/step_budget.rs` [new] | `StepBudget` state and tick logic, `STEP_BUDGET_EXIT_CODE` | n/a |
| `src/interpreter/gc.rs` | `GcPacer.poll` flag, `gc_safepoint_poll`, `gc_safepoint` | `poll` 33-34, `sync_poll` 92-94, `safepoint_pending` 161-167, `gc_safepoint_poll` 553-563, `gc_safepoint` 565 |
| `src/interpreter/mod.rs` | `Interpreter` field, public setters and getters, `pending_exit` doc | field 457, init 883, `run_program` 2385 |
| `src/interpreter/exec.rs` | tree-walker safepoint sites | 195, 466, 1792, 1815, 1840, 1862, 1982, 2262, 2533 |
| `src/interpreter/bytecode/vm.rs` | VM safepoint sites | `safepoint_with_empty_stacks` 1189-1199, `Op::Jump` 2426-2437, `Op::Safepoint` 2729-2737, exit return pattern 2679/2711 |
| `src/lib.rs` | public library entry for fuzz targets | `bench_eval` 89-117 |
| `src/cli.rs` | `--step-budget` flag, exhaustion message and exit code | `Cli` args 20-40, `exit_code_from_result` 98, `new_interp` 130 |
| `tests/step_budget.rs` [new] | black-box CLI tests, both execution modes | model on `tests/gc_stress.rs` |
| `docs/adr/2026-10-23-1200-step-budget-uncatchable-exit.md` [new] | decision record | format per `docs/adr/README.md` |
| `CONTEXT.md` | add the "Step Budget" term | near Safepoint/GC terms (~190) |
| `docs/agents/architecture-runtime.md` | one bullet on the budget and its safepoint contract | line 4 |
| `docs/perf/2026-10-23/1173-step-budget.md` [new] | unset-path measurement evidence | n/a |

## Steps

### 1. Add `StepBudget` state [new file]
- **File**: `src/interpreter/step_budget.rs` [new]; `mod step_budget;` in `src/interpreter/mod.rs`.
- **Change**: `pub(crate) const STEP_BUDGET_EXIT_CODE: i32 = 124;`. `pub(crate) struct StepBudget { limit: u64, used: u64, active: bool, exhausted: bool }` with `Default` (inactive). Methods:
  - `set(limit: Option<u64>)`: resets `used` and `exhausted`.
  - `is_active()`.
  - `tick(&mut self) -> bool`: returns true once exhausted. Semantics: `used += 1; if used > limit { exhausted = true }`, so budget N allows exactly N counted steps. Budget 0 exhausts at the first safepoint. Once exhausted it is sticky and stays true on every later tick.
  - `used()` and `exhausted()` getters.
- **Reuses**: none.

### 2. Plumb the budget into the safepoint poll with no unset cost
- **File**: `src/interpreter/gc.rs` (`GcPacer` 33-34, 92-94, 161-167; `gc_safepoint_poll` 553-563).
- **Change**:
  - Add `budget_active: bool` to `GcPacer`, included in `sync_poll` and in the `debug_assert_eq!` formula in `safepoint_pending`. Add `GcPacer::set_budget_active(bool)` that calls `sync_poll`. `mark_major`/`mark_minor` keep setting `poll = true`. `begin_collection`'s `sync_poll` calls preserve the budget bit through `sync_poll`.
  - Turn `gc_safepoint_poll` into `-> bool` (true = budget exhausted): `if self.gc.safepoint_pending() { return self.safepoint_slow(); } false`.
  - `#[cold] #[inline(never)] safepoint_slow`: `if self.step_budget.is_active() && self.step_budget.tick() { self.pending_exit = Some(STEP_BUDGET_EXIT_CODE); return true; }`, then `self.gc_safepoint(); false`. Exhaustion checks before collecting, and sticky ticks return true immediately.
  - Keep `gc_safepoint()` unit-returning and GC-only for `$262.gc` and `run_program`.
  - Latching `pending_exit` at the moment of exhaustion makes every boundary that cannot carry a `Completion` bail: `exec_try` (exec.rs:2746) converts to `Exit`, `drain_microtasks` (mod.rs:5414) stops, the VM's `CheckExit`/`checks_exit` handlers (vm.rs:975) and `dispose.rs:161` honor it. This covers an infinite loop reached inside an iterator `return()`, an inline `await` drain, or a Promise job.
- **Reuses**: `pending_exit` sink (mod.rs:457) and `Completion::Exit` (types.rs:246).

#### 2b. Event-loop ticks
- **File**: `src/interpreter/mod.rs` (`drain_microtasks` 5406, `run_due_timers` 5516), `src/interpreter/gc.rs`.
- **Change**: add `Interpreter::budget_tick_job(&mut self) -> bool` (`self.step_budget.is_active() && self.step_budget.tick()`, latching `pending_exit` on exhaustion). Call it before each `run_microtask` (the existing `pending_exit` early return then stops the drain) and before each timer callback in `run_due_timers`. A plain `is_active()` field read per job; no per-op cost.

### 3. Propagate exhaustion in the tree-walker
- **File**: `src/interpreter/exec.rs`, the nine sites listed above.
- **Change**: replace `self.gc_safepoint()` with `if self.gc_safepoint_poll() { <cleanup>; return Completion::Exit(STEP_BUDGET_EXIT_CODE); }`. At each site `<cleanup>` mirrors what the adjacent early `return Completion::Throw(..)` does: the `gc_root_value(&v)`/`gc_unroot_value(&v)` pairs in the loop sites (1792-1862, 1982, 2262), `call_stack_envs.pop()` in `exec_prepared_statements` (466), and the pending `gc_root_value(&v)` in `exec_for_in_of` (2533), which today unroots on its other early returns. Read each site's existing early returns before editing. Use the exit code from `pending_exit` rather than the constant when it is already latched.
- **Reuses**: `Completion::Exit` handling already present in `exec_try`, loops (`other => return other`) and iterator-close paths (`builtins/iterators.rs:4763-4850`).

### 4. Propagate exhaustion in the bytecode VM
- **File**: `src/interpreter/bytecode/vm.rs` (1189-1199, 2426-2437, 2729-2737).
- **Change**: `safepoint_with_empty_stacks` returns the `bool` from `gc_safepoint_poll()` (keep both `debug_assert!`s); the two call sites do `if safepoint_with_empty_stacks(..) { return Completion::Exit(interp.pending_exit.unwrap_or(STEP_BUDGET_EXIT_CODE)); }`. This is the same early-return shape as `Op::CheckExit` (2711); `run_chunk_with_var_prologue`'s unconditional root/env truncation handles cleanup (comment at 1170-1177). Verify that the generator and async chunk wrappers (vm.rs:1100-1115 and `eval/async_chunk.rs:142`, `eval/async_gen_chunk.rs:340`) already pass `Exit` through (they match `Completion::Exit`).
- **Back-edges that are not `Op::Jump` (added after review):** `Op::EndFinally` (vm.rs:2695) re-dispatches a stored `continue` target and can jump backward, e.g. `while (1) try { continue } finally {}` and `L: for (;;) try { continue L } finally {}`. Confirm by test whether such a loop reaches a safepoint; if it hangs, tick when `EndFinally` resumes at a target below `pc`. Also confirm the compiler never emits `CmpJumpIfFalse`/`LocalLocalCmpJump`/`LocalConstCmpJump` with a negative offset (grep `compiler.rs` for their emit sites); if one can, add the same back-edge tick to those ops.
- **Reuses**: the `Exit` return pattern at vm.rs:2679/2711.

### 5. Public setters and getters on `Interpreter`
- **File**: `src/interpreter/mod.rs` (field near 457, initializer near 883).
- **Change**: `step_budget: StepBudget` field. `pub fn set_step_budget(&mut self, limit: Option<u64>)` (also calls `self.gc.set_budget_active`), `pub fn step_budget_exhausted(&self) -> bool`, `pub fn steps_used(&self) -> u64`. Update the `pending_exit` doc comment: it is also written by budget exhaustion, so "always `None` unless node host floor" no longer holds.
- **Reuses**: none.

### 6. Library API for fuzz targets
- **File**: `src/lib.rs` (next to `bench_eval`, 89-117).
- **Change**: `pub enum ScriptOutcome { Completed, Threw, ParseError, StepBudgetExhausted }` (plain, `Send`, so it crosses `run_on_engine_stack`). `pub fn run_script_with_budget(src: &str, budget: Option<u64>, bytecode: bool) -> ScriptOutcome`: parse, `Interpreter::new()`, set `bytecode_enabled` and the budget, `run`, classify (`step_budget_exhausted()` takes precedence over the completion), and drain jobs (already done by `run`). The caller wraps it in `run_on_engine_stack`, as `fuzz_parse_bytes` does. Doc comment states the exhaustion semantics.
- **Reuses**: `parse_script` and the `bench_eval` structure (lib.rs:83-117).

### 7. CLI flag
- **File**: `src/cli.rs` (`Cli` 20-40, `new_interp` ~130, `exit_code_from_result` 98).
- **Change**: `#[arg(long = "step-budget", value_name = "N")] step_budget: Option<u64>`. Thread it through `new_interp` (add a parameter; update all callers) and call `interp.set_step_budget`. In `exit_code_from_result`, before the `pending_exit` branch, if `interp.step_budget_exhausted()` then `eprintln!("jsse: step budget exhausted after {N} steps")` and return `STEP_BUDGET_EXIT_CODE`. The REPL/prelude paths inherit it through `new_interp`; check the prelude path at cli.rs:278-280.
- **Reuses**: `exit_code_from_result`.

### 8. Docs
- **Files**: new ADR `docs/adr/2026-10-23-1200-step-budget-uncatchable-exit.md`; `CONTEXT.md`; `docs/agents/architecture-runtime.md`.
- **Change**:
  - ADR: context (ADR 0004's out-of-scope bullet), decision (fatal and uncatchable via `Exit` plus latched `pending_exit`, unit = counted safepoint, deterministic per mode, flag-poll design with zero unset cost), the not-covered list (expression-only recursion, native loops, agent threads), and the catchable-throw alternative with the reason it was rejected.
  - `CONTEXT.md`: a "Step Budget" entry.
  - `architecture-runtime.md`: one bullet saying every new loop or statement-list construct must keep a safepoint through `gc_safepoint_poll()` and propagate its `bool`.
- **Reuses**: ADR README naming scheme.

### 9. Unset-cost measurement
- **File**: `docs/perf/2026-10-23/1173-step-budget.md` [new].
- **Change**: record before/after on `benchmarks/scripts/bench_loop.js`, `bench_fib.js` and `bench_opmix.js`, in both modes. Method per `docs/agents/execution-counters.md`: build `release-nolto --features perf-counters` on `main` and the branch and diff the `PERF`/`OP` blocks (they must be identical, since no new ops or work were added). For instruction cost, `valgrind` is not installed here, so use `perf stat -e instructions:u` (or callgrind if present on the implementer's host) on the default release build. Report the delta and the timing method. Never compare instrumented timings. Target: no measurable change in the unset path (≤0.5% instructions).

## TDD slices

1. **Budget counter unit.** Test: `#[cfg(test)]` module in `src/interpreter/step_budget.rs`. Behavior: `set(Some(3))` makes the 1st-3rd ticks return false and the 4th and later return true (sticky); `set(Some(0))` exhausts on the first tick; `set(None)` is inactive; `set` resets `used`/`exhausted`. Production: step 1.
2. **Pacer poll gating.** Test: extend the `safepoint_poll_flag_tracks_requests_and_stress_mode` neighbourhood in `src/interpreter/gc.rs` (~1940). Behavior: `set_budget_active(true)` raises `poll`, `false` lowers it, and the `safepoint_pending` debug assertion holds with the budget bit alongside minor/major/stress. Production: step 2, `GcPacer` part.
3. **Tree-walker infinite loop, red to green.** Test: `tests/step_budget.rs` [new] `while(1){}` and `for(;;){}` with `--no-bytecode --step-budget 1000` expect exit 124 and stderr containing `step budget exhausted`. Production: steps 1, 2 (poll slow path), 3, 5, 7.
4. **Bytecode infinite loop.** Test: same file, default mode. Cover `while`, `do..while`, `for(;;)`, `for-in`, `for-of` over an infinite generator, a loop inside a called function, plus the `EndFinally` back-edge cases `while (1) try { continue } finally {}` and `L: for (;;) try { continue L } finally {}`. Production: step 4. Assert via `--perf` nothing; just the exit code.
5. **Uncatchable (discriminating).** Exit code and stdout alone cannot detect a swallowed `Exit` because exhaustion is sticky and re-fires at the first statement of any `catch`/`finally`. So the check is `steps_used() == limit + 1` exactly after exhaustion, asserted through the library API (slice 8) for each slice-6 boundary (`forEach`, `sort`, getter, Proxy trap, iterator `return()`, async after `await`); any overshoot means a boundary swallowed the `Exit`. CLI test: `try { for(;;){} } catch(e){ print('caught') } finally { print('fin') }` plus the nested form `for(;;){ try { for(;;); } catch(e){} }`, both modes. Expect exit 124 and no `caught`/`fin` on stdout. Production: no new code; locks in the `pending_exit`/`Exit` design.
6. **Boundaries that cannot carry a completion.** Tests (both modes): infinite loop inside `[1].forEach(cb)`, `[3,2,1].sort(cmp)`, a getter, a Proxy trap, a `toString` coercion, a Promise executor and `.then` job (statement-bodied `function f(){Promise.resolve().then(f)} f()` AND expression-bodied `const f = () => Promise.resolve().then(f); f()`), `setTimeout` self-rescheduling and `setInterval(() => 0, 0)` (expression-bodied), an async function after `await`, a generator body, an iterator `return()` called by `break`, and `eval`/`new Function` bodies. Expect exit 124 within the budget in every case. Production: fix any native that swallows a non-`Throw` abrupt completion that the tests expose (audit candidates: natives that map `Completion` into `Result<_, JsValue>`).
7. **Budget not hit and determinism.** Tests: a finite 100-iteration loop under a large budget completes with exit 0 and the same stdout as without a budget. Two runs of an exhausted `console.log(i)` loop produce byte-identical stdout (determinism). Budget `0` exhausts immediately. Production: none beyond slices 3-4.
8. **Library API.** Add `ScriptOutcome::StepBudgetExhausted` carrying `steps_used` or expose a variant of `run_script_with_budget` returning `(ScriptOutcome, u64)` so tests can assert the exact overshoot-free count. Test: `lib_tests` in `src/lib.rs`, under `run_on_engine_stack`: `run_script_with_budget("while(1){}", Some(10_000), true/false)` returns `StepBudgetExhausted`; `"throw 1"` returns `Threw`; `"let ="` returns `ParseError`; `"1+1"` with `None` returns `Completed`; the in-process interpreter is reusable in a loop (fresh interpreter per call, run 100 times without leak or abort). Production: step 6.
9. **Docs and measurement.** Steps 8 and 9, then the gates below.

## Test surface

- `cargo build --release`, then `cargo test --profile release-nolto` (unit and the new `tests/step_budget.rs`). The new integration tests use `env!("CARGO_BIN_EXE_jsse")` as `tests/gc_stress.rs` does.
- `./scripts/ensure-hooks.sh` before work and before committing; `./scripts/lint.sh`.
- Full `uv run python scripts/run-test262.py` (no `--update-baseline`) and compare against `origin/main:test262-pass.txt`. Also `uv run python scripts/run-custom-tests.py`.
- `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py test262-extra/` normal and `--no-bytecode`, plus one budget test under `JSSE_GC_STRESS=1` (the safepoint changes must not disturb rooting).
- `cargo build --profile release-checked` and run `tests/step_budget.rs` scenarios plus test262-extra against it to exercise the root-depth `debug_assert`s on the new early-return paths (the checked binary has debug-sized recursion limits; keep recursion-limit tests on the plain release build).
- Targeted test262 directories: `test262/test/language/statements/{while,do-while,for,for-in,for-of,try,labeled}/`, `test262/test/language/expressions/{generators,async-function}`, `test262/test/built-ins/{Promise,Array/prototype/sort}`. With no budget set the engine behavior must be unchanged.
- `scripts/` Node-compat shims and library harnesses are untouched, and they never pass `--step-budget`.

## Regression risk

- **Baseline (`test262-pass.txt`)**: with no budget, the only runtime changes are the `bool` return from `gc_safepoint_poll`, the tree-walker sites moving from the unconditional `gc_safepoint()` to the poll form, and the extra flag term in `sync_poll`. The tree-walker change is the one to scrutinise: `gc_safepoint()` currently calls `begin_collection` every time, and the poll form skips that when `poll` is false. Any behavior relying on `begin_collection`'s side effects at unneeded safepoints (stress counter, `cfg(test)` `safepoint_calls`) must be rechecked; stress mode keeps `poll` true so it is unaffected. Expected baseline movement: none.
- **Hot paths**: `exec_statements`/loop sites in the tree-walker and `Op::Jump`/`Op::Safepoint` in `run_chunk_inner`. The VM dispatch loop is sensitive to codegen (see the named-access note in `docs/agents/architecture-vm.md`: inlining changed dispatch codegen and cost 2-4%). Keep the new branch inside the existing `#[cold]` slow call and verify with the step 9 measurement. If the `bool` check regresses the VM, fall back to checking `interp.pending_exit.is_some()` only inside the slow path's call result, or restructure so the VM tests a single combined condition.
- **GC rooting**: the new early returns must keep `gc_temp_roots` and `gc_bytecode_roots` balanced (asserted by `gc_assert_root_depth` and the chunk-exit assert; `Exit` is excluded from the VM chunk-exit assert). The release-checked and GC stress runs cover this.
- **Root balance at every user-code boundary (most likely real bug):** the budget makes `Exit` reachable from inside every native that calls user code. A native that returns the abrupt completion without truncating its `gc_temp_roots` frame trips `gc_assert_root_depth`. Run *every* slice-6 boundary test, not just the loop tests, on `release-checked` and under `JSSE_GC_STRESS=1`.
- **Uncatchability depends on every boundary honoring `Exit`/`pending_exit`**: natives that convert completions to `Result<_, JsValue>` could drop an `Exit` and keep looping (the loop's next safepoint would tick again and re-exhaust, so it still terminates, but it may run extra user code). Slice 6 enumerates the boundaries. Because exhaustion is sticky and ticks return true at every later safepoint, a swallowed `Exit` terminates at the next safepoint. This is the safety net.
- **Exhaustive matches**: no new `ObjectKind` or `Completion` variant is added, so no exhaustive-match churn.
- **Node-compat harnesses**: `pending_exit` doc and semantics change only when a budget is set. `--node` runs never set one.

## Out of scope

- Counting function entries, native-loop iterations, regex steps, or allocation (a possible follow-up for expression-only recursion and native loops).
- A new libFuzzer target or a `differential` in-process mode (follow-up under #1168).
- Budget inheritance by Atomics agent threads.
- A catchable-exhaustion mode or a wall-clock timeout.
- Refactoring the safepoint call sites beyond what exhaustion propagation needs; no formatting or unrelated cleanups.
- Updating `test262-pass.txt` (a `main`-branch operation).
- Editing ADR 0004 (it is frozen history); the new ADR supersedes its "step budget" out-of-scope bullet by reference.
