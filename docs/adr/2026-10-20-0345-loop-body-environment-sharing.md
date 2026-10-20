# Loop bodies share the loop's environment when nothing can tell

Status: accepted (#1132); amends decision 6 of
ADR-2026-10-13-0632 (block environments in the bytecode VM).

## Context

ADR-2026-10-13-0632 made `for (let ...)` reuse one head environment when no
closure or `eval` can observe it, but the body block of such a loop still ran
`PushEnv`/`PopEnv` on every iteration, and so did the body of every `while`,
`do`-`while`, `for-in` and `for-of`. For `for (let i...) { let t = i + 1; s +=
t; }` that was about half of the loop's instructions (1,254 Ir per iteration
against 759 with the body's bindings in the loop environment).

## Decision

1. **One environment per loop entry.** A loop whose body is a block declaring
   `let`/`const` opens its scope before the loop (`compiler/loop_scope.rs`,
   `open_loop_scope`): the head's own bindings (a `for (let ...)` or
   `for (const x of ...)` head) and the body block's bindings in one `PushEnv`.
   The body's statements compile with `owns_lexicals`, so each declaration is an
   `InitLexical` into that environment on every iteration, which overwrites the
   previous iteration's value. No new op is needed; `break`/`continue` leave the
   same number of environments as before because the scope is opened before the
   loop frame records its depth.
2. **When it is unobservable.** All of these must hold, or the body keeps its
   own `PushEnv` per iteration:
   - nothing in the body can capture the environment (`CaptureWalk`: no function,
     arrow, class, `with` or `eval` mention) and the loop does not need
     CreatePerIterationEnvironment (`may_capture_iteration_scope`);
   - the head, test and update (which now run inside the shared environment)
     mention none of the body's names, and no closure sits in them;
   - every mention of a body binding follows its declarator in source order, so
     a read can never see the previous iteration's value where a fresh
     environment would throw the TDZ `ReferenceError`; a declarator's own
     initializer and the declarators after it count as before-initialisation;
   - every top-level `let`/`const` is one the compiler lowers to `InitLexical`
     (an identifier, or a plain object pattern with an initializer); `using`
     declarations, tree-walker hand-offs for array patterns and class or
     function declarations all keep the per-iteration environment.
3. **Where.** Function goal only. The Script goal keeps its per-node fallback for
   blocks that declare anything.
4. **Analysis lives with the AST scan.** `CaptureWalk` moved to
   `ast/capture_walk.rs` and can collect the identifiers it reaches
   (`CaptureWalk::collecting`), so the capture test and the name test share one
   exhaustive walk.

## Consequences

- `PushEnv` runs once per loop entry for such loops; the remaining distance to
  the equivalent `var` loop is `InitLexical` against `StoreLocalPop`.
- A binding's last value stays reachable until the loop exits instead of until
  the iteration ends. No closure, `eval` or `with` can reach it, and when an
  unreachable object is collected is implementation-defined (WeakRef and
  FinalizationRegistry may retain objects longer), so nothing conforming
  depends on the difference.
- Nested blocks inside the body (`if (...) { let x }`) still push their own
  environments; folding those in needs a per-name shadowing analysis and is left
  for a follow-up.
