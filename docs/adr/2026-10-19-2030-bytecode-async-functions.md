# Async function bodies as suspending bytecode chunks

Status: accepted (#1105, first stage); extends ADR-2026-10-17-0640 (sync
generators) and amends the async-function notes in `AGENTS.md`.

## Context

An async function body was always lowered by `generator_transform.rs` into a
state machine whose payloads run on the tree-walker, with the per-resume
reconciliation of the scope stack and the for-of stack that the sync generator
driver paid before #1103. The untimed setup of `async-fs` (a fill loop inside
an async function) spent 5.6 G of its 8.4 G Ir there against boa's 1.7 G.

## Decision

1. **`await` is a second suspension of the same op.** `Op::Yield` with a zero
   skip offset is an `await`: it has no return stub, because a resumption is
   only a value (`Resume::Next`, pushed as the op's result, or not at all when
   the result is discarded) or a rejection (`Resume::Throw`, raised at the op,
   where the handler table finds the enclosing `try`). The VM is unchanged;
   a driver tells the two apart by the skip operand
   (`compiler/suspend.rs`). Nothing is added to `run_chunk_inner`.
2. **Eligibility is the generator's.** `compile_async_body` is the Function
   goal with `await` lowered natively; it is `Ineligible` for `for await`
   (already poison), `using`/`await using` declarations, and every
   construct the Function goal keeps on the tree-walker (`with`, direct
   `eval`, `super`, `new.target`). An `await` inside a node the compiler hands
   to the tree-walker (a destructuring default, a template, a computed key)
   would not be able to suspend, so such a body keeps the state machine. The
   verdict is memoised per Body; `--no-bytecode` keeps every async function on
   the state machine.
3. **The activation lives on the Rust stack while it runs.** The call path
   still binds the parameters and builds the arguments object; a hosted body
   then instantiates its declarations (`instantiate_hosted_body`, shared with
   sync generators) and runs the chunk (`run_async_activation`) up to the
   first `await` with no scheduler involvement. A body that never awaits
   never touches `Interpreter::async_activations`. At an `await` the
   activation (chunk, frame, environments, result capability) is moved into
   that map and the continuation is registered with the state machine's own
   `await_promise_resolve` + `await_then_internal` as
   `ReactionTarget::AsyncResume(id)`, so tick counts are unchanged;
   `async_function_resume` finds the id in `async_activations` and resumes.
4. **Roots.** A parked activation is traced from
   `collect_async_activation_roots` (frame environments, operand values,
   result promise); while it runs, its environment is on `call_stack_envs`,
   its operands on `gc_bytecode_roots` and its capability is temp-rooted.
5. **Tail calls** are suppressed for the duration of a run
   (`tco_suppress_depth`): a tree-walked hand-off inside the chunk must not
   return a `TailCall` to a driver that cannot trampoline it.

## Consequences

- Operands that precede an `await` are evaluated before the function suspends
  and keep their values across it, natively; the state machine reproduces this
  with temps.
- A latent state-machine bug is fixed on the way: a lowered scope entered
  during one resumption reached the scheduler's saved state only at the next
  suspension, so a collection inside a call made by that step could free what
  only the scope held (`pin_driver_env`). Hosted calls into such a body made
  it observable at `JSSE_GC_STRESS=7`.
- Async generators, `yield*` and `for await` remain on the state machine.
