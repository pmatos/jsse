# Sync generator bodies as suspending bytecode chunks

Status: accepted (#1031); amends ADR-2026-10-12-2214 (a `yield` the scan meets
no longer keeps a generator body on the tree-walker when the body can be
hosted) and the generator-state-machine notes in `AGENTS.md`.

## Context

A sync generator body was always lowered by `generator_transform.rs` into a
state machine whose state payloads run on the tree-walker: every `next()` ran
`generator_next_state_machine_impl`, which reconciled the lexical scope stack
and the for-of stack against the state's static depth, evaluated the
terminator expression through `eval_operand`, and executed the (yield-free)
payload through `exec_body_inner`. On lazy-collections that was 8.97 G Ir with
63% of the run's AST work units in generator payloads and terminators, 608 Ir
of driver bookkeeping per state dispatch, and name resolution through string
keys (`resolve_identifier_hinted::<String>`, `walk_scope_chain::<String>`).

## Decision

1. **A generator body is a bytecode chunk that suspends.** `Op::Yield`
   returns `Completion::Yield(value)` from `run_chunk_inner` after saving the
   program counter and the block environments; the operand stack and the
   reference stack are the generator object's own vectors, so nothing else
   needs saving. `run_generator_chunk` re-enters the loop at the saved
   position, re-rooting the operands and environments (`gc_bytecode_roots`,
   `call_stack_envs`) that a running frame keeps and a suspended one does not.
   The suspended frame is traced through `IteratorState::BytecodeGenerator`
   (`collect_iterator_state_roots`).
2. **The three resumptions are encoded at the yield.** `Op::Yield` is followed
   by a stub, the exit a `return v` statement would emit at that position
   (`compile_jump(Terminal::Return)`). `next(v)` pushes `v` and jumps over the
   stub; `return(v)` cuts the operand stack back to the persistent height
   recorded in the op (iterators and finalizer records stay, expression
   temporaries go) and runs the stub, so enclosing `finally` blocks and
   `for-of` iterator closes run through the cleanup machinery that already
   handles `return`; `throw(e)` raises `e` at the op (`resume_at_handler`).
   A yield whose sent value is unused (`yield x;`) sets a discard flag and
   pushes nothing.
3. **Eligibility is decided by the compiler, per Body.** `compile_generator_body`
   is `compile_function_body` with the generator flag. It is `Ineligible` for
   `yield*`, `using` declarations, and anything the Function goal already
   keeps on the tree-walker (`with`, direct `eval`, `super`, `new.target`); and a
   node the compiler must hand to the tree-walker (`EvalAst`/`ExecAst`, a head
   binding pattern) may not contain a `yield`, since it could not suspend, so
   such a body keeps the state machine. Generator bodies have no tail position
   (`no_tail` is set for the whole chunk). The verdict is memoised per Body in
   `bytecode_memo`; `--no-bytecode` keeps every generator on the state machine.
4. **Creation is unchanged.** The call path still binds parameters, builds the
   body environment and creates the generator object before returning it;
   the chunk's `var`/lexical/function declarations are instantiated at that
   point (`bytecode_generator_state`), and the body starts at the first `next()`.
5. **One resume function.** `bytecode_generator_resume` handles `next`,
   `return` and `throw` for a `BytecodeGenerator`, with the same
   start/suspended/executing/completed protocol as the spec's generator
   states; `generator_step_value` (for-of, spread, `Array.from`) parks
   `(value, done)` in `raw_iter_step` instead of building a result object.

## Consequences

- Evaluation order of operands that precede a `yield` is now spec-correct for
  these generators (`f(a(), yield b())` evaluates `a()` before suspending); the
  state machine, which re-evaluates a state's statements on resume, still gets
  this wrong, as the extra test notes.
- A body that mixes constructs the compiler cannot host with ordinary
  statements stays wholly on the state machine; `yield*` is the common one.
- The state machine remains the implementation for async generators and async
  functions.
