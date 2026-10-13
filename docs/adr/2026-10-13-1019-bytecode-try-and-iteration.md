# try/catch/finally, for-in and for-of in the bytecode VM

Status: accepted (#960); amends ADR-2026-10-12-2214 (the list of constructs
that stay on the tree-walker) and ADR-2026-10-13-0632 (decision 7, which left
`try` and the iteration statements as `ExecAst` units).

## Context

After block environments (#958) and literals/closures (#959) the constructs
that still sent whole loops to the tree-walker were `try` and `for-in`/`for-of`.
A `try` that wraps a hot loop (gbemu's decode loop) or a `for-of` whose body is
the interesting work (UniPoker, Babylon) ran as one `ExecAst` unit, with every
statement inside on the tree-walker: 5.7M work units in gbemu, 940k in
UniPoker.

## Decision

1. **A handler table, not handler ops.** `Chunk.handlers` lists protected
   code ranges, innermost first: a `Throw` from an op in `[start, end)` resumes
   at `target` with the operand stack cut back to `stack_height` values and the
   block environments to `scope_depth`, the thrown value pushed on top. The
   non-throwing path pays nothing at entry and exit. `run_chunk_inner` leaves
   the dispatch loop with the thrown value, `resume_at_handler` (cold,
   out of line) picks the handler, and a chunk with no handlers returns the
   throw exactly as before. The faulting op is found from `pc - 1`, which lies
   inside it whatever operands it had decoded. The GC roots of the discarded
   operands are cut back with the stack (`gc_bytecode_roots` is truncated to
   the roots still on the stack, so a leaked temporary cannot survive).
2. **`finally` is one block entered with a completion record.** The
   finalizer is compiled once. Every way into it pushes `[value, kind]`:
   kind 0 normal completion, 1 a throw (from the handler pad), `n >= 2` an
   exit routed through it (a `return` with its value, or a `break`/`continue`
   to an outer target). `EndFinally table` pops the record and either falls
   through, rethrows, or jumps to the exit's *pad*, the rest of that exit
   emitted behind the finalizer (closing an enclosing iterator, entering an
   outer finalizer, then the `Return` or the jump). The kinds are allocated
   per finalizer and deduplicated by (terminal, has-value). An abrupt
   completion of the finalizer itself drops the record (it is a persistent
   slot, below) and replaces the pending one, as the spec says.
3. **Persistent operand slots.** A for-in/for-of iterator and a finalizer's
   completion record live on the operand stack across statements, so they are
   rooted by `push_value` like every other operand and a handler or an escape
   table drops them by truncating the stack. The compiler tracks them as
   `stack_base`; `Chunk.persistent_slots` bounds the debug assertion that the
   stack is otherwise empty at a safepoint.
4. **A cleanup stack replaces ad hoc unwinding.** `break`, `continue` and
   `return` all go through `emit_unwind`, which walks the open cleanups
   (`Iter`, `Record`, `Finally`) from the innermost, leaving block
   environments, dropping slots, closing iterators (`IterClose`, which
   removes the iterator from under a pending return value) and routing
   through finalizers. With no cleanups open it emits exactly what the
   previous `PopEnv`/`Jump`/`Return` sequences did.
5. **for-of.** `GetIterator` (the tree-walker's `get_iterator`),
   `IterStep` (`iterator_step_value`, with the allocation-free intrinsic
   steps of #978) and, for the exits, `IterClose` (IteratorClose with a normal
   completion; an error from `return()` or a non-object result propagates).
   An abrupt exit of the head binding or the body is a handler range whose pad
   is `IterCloseThrow` (IteratorClose with the throw completion, where an
   error from `return()` is discarded). The step is outside that range: an
   error from `next()`, `done` or `value` does not close the iterator
   (IteratorStepValue marks it done). A `break` lands on a pad that closes; an
   exhausted iterator is dropped by the step and skips it.
6. **for-in** reuses the enumerator object the generator state machines
   already build (`for_in_head_iterator`: a snapshot of the keys, a nullish
   value yields an exhausted enumerator) and steps it without result
   objects (`for_in_enumerator_step`, the same code `%ForInIteratorPrototype%.next`
   runs, including skipping keys deleted before they are visited). Its
   prototype chain is null, so nothing is looked up on exit and for-in needs
   no close handler, only a slot to drop.
7. **Heads.** `let`/`const` identifier heads enter a block environment (the
   head's names are in their TDZ while the iterated expression is evaluated,
   in a scope of its own). When nothing in the loop can capture the iteration
   scope (`may_capture_iteration_scope`, the test `exec_for_of` uses to share
   one environment) a simple head's scope is entered once per loop and each
   iteration only `InitLexical`s the binding; otherwise a fresh environment
   per iteration. `var` and assignment-target identifiers compile to the
   ordinary name stores. Destructuring and member-expression heads run
   through `BindHead`, which calls the tree-walker's `bind_pattern` /
   `assign_to_for_pattern` on the value, so pattern semantics have one
   implementation. `using` heads, a for-in initializer and an expression
   target (`for (f() in o)`) stay on the tree-walker.
8. **Catch.** The parameter is bound in a block environment of its own
   (`simple_catch` marks an identifier parameter, B.3.4); the clause body is
   an ordinary block inside it. A parameter pattern is bound by `BindHead`
   in TDZ.
9. **Tail calls.** Calls in a `try` block, and in a `catch` block whose
   `finally` has yet to run, are not in tail position
   (sec-static-semantics-hascallintailposition): the compiler emits `Call`
   there, and `ExecAst` carries a flag so a statement handed to the tree-walker
   inside such a region runs with the tree-walker's own `tco_suppress_depth`
   raised. A `return f()` in a finalizer or a for-in body stays a tail call;
   the VM drops the slots below it when it leaves with the `TailCall`.
10. **Hand-offs that would skip a cleanup.** A statement left to the
    tree-walker (`ExecAst`) inside a region that has code to run on the way
    out cannot be allowed to complete with a `return`, or a `break`/`continue`
    leaving that region: the VM would leave without running the finalizer or
    the iterator's `return()`. The compiler checks this per hand-off
    (`collect_escaping_jumps`) and, when it would, rolls back to the
    enclosing construct, which then stays on the tree-walker as a whole
    (`completion leaves a cleanup region`). Escapes that only cross slots
    with nothing to run (for-in, a completion record) are handled by the
    escape table, which now also records the stack height to cut back to.
11. **`__host_exit`.** A `try` handler honours a latched exit (`pending_exit`)
    like `exec_try` does: the exit stays uncatchable and skips `finally`
    (`CheckExit` at the end of the block, and the handler lookup).
12. **Script goal unchanged.** `try`, for-in and for-of still bail the whole
    body there.

## Consequences

- `FALLBACK statement:Try` / `statement:ForIn` / `statement:ForOf` read 0
  on gbemu, UniPoker and Babylon; their tree-walker work units fall from
  5.7M, 940k and 46k to 2.7k, 330 and 44k (Babylon's remainder is template
  literals, nested tail calls and destructuring declarations).
- The tree-walker's for-in assigned a destructuring *assignment* target
  (`for ([a, b] in o)`) as a new `let` binding of the loop scope instead of
  updating the existing bindings; it now uses the assignment path the
  for-of loop always had.
- `Interpreter::call_function_inner_impl`'s `CallTarget::User` now carries
  its six booleans as one flags byte. The previous layout was scalarised and
  re-packed by LLVM differently each time unrelated code moved
  (+186 instructions per call on this change before the fix); the byte keeps
  the call path's instruction count at or below what it was.
- A `try` or for-of containing a statement the compiler cannot lower *and*
  that completes with a `return`/`break`/`continue` out of the region stays
  entirely on the tree-walker. The `FALLBACK` reason names it.
