# Step budget: deterministic, opt-in, fatal and uncatchable

Status: accepted (#1173); lifts the "step budget" out-of-scope bullet of ADR 0004.

## Context

Fuzz targets run `jsse` as a subprocess with a wall-clock kill (ADR 0004)
because nothing stops a runaway script in-process. Running generated scripts
in-process needs a reproducible way to end infinite loops and heavy scripts.
ECMAScript has no step-limit concept; this is a host resource limit like the
recursion `RangeError` guard, so no spec clause is involved and nothing is
observable when no budget is set.

## Decision

1. **One step is one counted safepoint**: a statement boundary or a loop
   back-edge (`Op::Safepoint`, negative-offset `Op::Jump`, and the tree-walker's
   `gc_safepoint_poll()` sites), plus one per microtask job and per timer
   callback (expression-bodied callbacks have no statement or back-edge, so
   `const f = () => Promise.resolve().then(f)` would otherwise spin forever).
   The count is deterministic per execution mode for scripts that read no
   clocks or entropy. It differs between bytecode and `--no-bytecode`: the
   compiler drops the safepoint of a statement that compiles to nothing.
2. **Exhaustion is fatal and uncatchable.** It rides `Completion::Exit` and the
   `pending_exit` sink that `__host_exit` already uses: skipped by `catch`,
   `finally`, disposers, iterator `return()` and promise reactions. Exit
   status 124 (`STEP_BUDGET_EXIT_CODE`), stderr
   `jsse: step budget exhausted after N steps`. With `--node`, `__host_exit(124)`
   shares the code; callers tell them apart via `step_budget_exhausted()`.
3. **No cost when unset.** The budget bit is folded into `GcPacer.poll`, so the
   safepoint stays one flag test; the tick lives in the existing `#[cold]` slow
   path. `Interpreter::set_step_budget(Some(n))` raises the flag.
4. **Library API**: `Interpreter::{set_step_budget, step_budget_exhausted,
   steps_used}` and `run_script_with_budget(src, budget, bytecode)` returning
   `(ScriptOutcome, steps)`, for in-process fuzz targets. CLI: `--step-budget N`.

## Rejected: a catchable throw

A catchable `RangeError` is unsound for the purpose: `for(;;){ try { for(;;); }
catch(e){} }` swallows it forever and a `finally` can run unbounded work. A
sticky re-throw at every later safepoint still lets `finally` run user code and
touches every `catch` site.

## Not covered

Work that never reaches a safepoint is outside the budget: exponential
recursion through expression-bodied arrows (the depth guard still applies),
native loops over a user iterator whose `next` is an expression arrow
(`new Set({[Symbol.iterator]: () => ({next: () => ({value: 1, done: false})})})`),
catastrophic regex backtracking, huge native loops (`"x".repeat(2**28)`).
Counting function entries or native-loop iterations is a follow-up, as is a
watchdog for the iterator case. Atomics agent threads build their own
`Interpreter` and do not inherit the budget.
