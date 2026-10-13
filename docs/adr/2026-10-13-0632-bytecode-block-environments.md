# Block environments in the bytecode VM

Status: accepted (#958); amends the scope rule of
ADR-2026-10-12-2214 (decision 4, "Scope rule").

## Context

ADR-2026-10-12-2214 let the compiler hand single nodes back to the
tree-walker, but the VM had no environments of its own: a block, `switch` or
loop head that declared a binding (`let`/`const`/class/function) ran as one
`ExecAst` unit, and with it every statement inside, however hot. Modern-syntax
loop bodies (`for (...) { const v = ...; ... }`) therefore stayed on the
tree-walker: UniPoker 28%, Babylon 19% and raytrace 99.97% of their work
units.

## Decision

1. **Environments are VM state.** `run_chunk_inner` keeps the frame
   (function) environment fixed and a stack of block environments
   (`scopes`) the chunk has opened. The current environment is the innermost
   block environment, else the frame's. Every environment use outside the
   local slot ops (`LoadName`, `ResolveName`, `LoadThis`, `EvalAst`,
   `ExecAst`, `InitLexical`, ...) goes through it, so a hand-off node and the
   closures it creates see the block's bindings exactly as the tree-walker
   would have given them.
2. **Three ops.** `PushEnv scope` creates a child of the current
   environment and instantiates the block's declarations
   (BlockDeclarationInstantiation: closures for block function declarations,
   then `let`/`const`/class bindings in their TDZ, from a per-chunk
   `BlockScope` table of pre-interned atoms). `PopEnv n` leaves `n`
   environments. `CopyIterEnv` is CreatePerIterationEnvironment: it replaces
   the innermost environment with a fresh sibling holding copies of its
   bindings.
3. **Rooting.** Each block environment is also pushed on
   `interp.call_stack_envs` (PushEnv pushes, PopEnv pops, `CopyIterEnv`
   replaces the top), because the VM's own stack is invisible to the
   collector. `run_chunk_with_var_prologue` truncates `call_stack_envs` to
   its entry length unconditionally on exit, which is all `return`, `throw`
   and `TailCall` from inside a block need.
4. **Static depth.** The compiler tracks the open block environments
   (names per environment, in `PushEnv` declaration order). A name found in
   one shadows the frame's locals. Each `LoopFrame` records the depth at its
   break/continue targets; `break`/`continue` emit `PopEnv` for the
   difference before jumping, and `EscapeEntry` carries that depth so a
   `break`/`continue` completion leaving an `ExecAst` unwinds the VM to it.
5. **Slot hints address an environment.** A local-op slot operand with the
   high bit clear names a slot in the frame environment, as before. With it
   set, bits 8-14 index the chunk's open block environments (outermost
   first) and bits 0-7 are the slot. Still only a hint, verified by atom
   identity; the miss path resolves by name from the *current* environment.
   Names beyond the encoding (more than 128 nested environments, or 256
   names in one) get no hint and take the by-name path.
6. **Per-iteration copies only when observable.** `for (let ...)` copies
   its head environment per iteration only when
   `ForStatement::may_capture_iteration_scope()` says a closure, `eval` or
   class could observe it (the same test `exec_for` uses); otherwise one
   environment serves the whole loop.
7. **`switch`.** The case block's environment is entered after the
   discriminant is evaluated, selectors run inside it, and `break` leaves
   it. A case clause that declares a function keeps the whole `switch` on
   the tree-walker (`block-scoped declaration`): the tree-walker hoists
   those with its own `var`-kind rules. `using`/`await using` blocks stay
   there too: their environment owns a disposal stack.
8. **Environment reuse.** A block environment nobody captured
   (`Rc::strong_count == 1` once the VM drops its references) goes back to
   the function-environment pool and is reset on reuse.
9. **Script goal unchanged.** It still bails whole-body on a block that
   declares anything.

## Consequences

- A function whose loops declare `let`/`const` (or `for (let ...)` heads) is
  VM-resident end to end; `FALLBACK block-scoped declaration` and
  `lexical declaration` rows read 0 on UniPoker, Babylon and raytrace.
  What remains of `ast_units_in_fallbacks` there is `for-of`/`for-in`,
  `try`, array/object literals and closure creation (#959, #960).
- Destructuring declarations (`const {a} = o`) and class declarations in a
  block run as single `ExecAst` statements inside the block's environment.
- Fixes found on the way, both in the tree-walker's `switch`: the CaseBlock's
  `let`/`const` now enter their TDZ at switch entry (a read before the
  declaration used to resolve to an outer binding instead of throwing), and
  the discriminant and the case environment are rooted while selectors and
  clauses run (a collection inside a clause freed values held only by a
  case-local binding).
