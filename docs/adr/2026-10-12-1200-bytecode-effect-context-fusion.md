# Bytecode effect-context fusion and inline number ops

Status: accepted (#903)

## Context

The bytecode VM dispatched four ops for a statement-context assignment
(`ResolveName`/`LoadLocal`, the right-hand side, a store, `Pop`), a separate
`Pop` after every statement-level `i++`, a `LoadConst` before every literal
operand, a `Lt` then a `JumpIfFalse` for every branch on a comparison, and
re-dispatched every binary op a second time inside `eval_binary`. After #898
the `arith` loop of `benchmarks/scripts/bench_opmix.js` still cost 43 counted
dispatches per iteration, and dispatch overhead (`run_chunk_inner` self cost,
`push_value`, double binary-op dispatch) dominated its profile.

## Decision

1. **Effect context.** `Compiler::compile_effect` compiles an expression whose
   value is discarded and leaves the operand stack unchanged. It is used for
   function-goal expression statements, `for` initialisers and updates, `var`
   initialisers, non-final comma elements and `void` operands. Script-goal
   expression statements are *not* effect context: their value is the
   completion value (`SetCompletion`).
2. **Fused stores.** In effect context an identifier assignment ends in
   `StoreLocalPop` (frame local) or `StoreResolvedNamePop` (by name). The
   reference is still created by `ResolveName` *before* the right-hand side, and
   the value stays on the operand stack, rooted, through the store (a setter or
   proxy trap can collect) and is popped afterwards.
3. **Update discard bit.** Bit 2 of the `UpdateLocal`/`UpdateName` mode byte
   means "do not push the result". It is a bit, not new opcodes, because the
   operation is otherwise identical. ToNumeric and PutValue still run, so
   coercion side effects and errors are unchanged.
4. **Constant folding.** A number-literal right operand folds into
   `BinConst` (`[.., l]` → `[.., l op K]`), and `Binary(op, local, literal)`
   into `LocalBinConst` (a `LoadLocal` and `BinConst` in one dispatch). The
   operator is stored as the byte of the corresponding binary `Op`.
5. **Compare + branch.** A comparison used as an `if`/`while`/`for`/`do-while`
   test or a `?:` condition compiles to `CmpJumpIfFalse` (comparison byte, then
   the i16 offset last so `patch_jump` is unchanged). A comparison result is
   always a Boolean, so ToBoolean is the identity. `local < literal` keeps
   `LocalBinConst` + `JumpIfFalse`, already two dispatches.
6. **Inline number ops.** Each binary-op arm of the dispatch loop (one arm per
   op, via a local macro) peeks the top two stack values and, when both are
   Numbers, computes the result with the same `number_ops` calls as
   `eval_binary`'s own Number fast path and overwrites the left operand in
   place. Numbers are never rooted, so nothing is unrooted. Everything else
   goes through the non-inlined `binary_slow`, which keeps both operands rooted
   across `eval_binary` and unroots them (right, then left) afterwards. A
   non-Number `LocalBinConst` operand is rooted before the slow path runs: an
   object operand's `valueOf` may rebind the local and collect.
7. `push_value`, `pop_value`, root/unroot helpers and the operand decoders are
   `#[inline(always)]`; `OP_SLOTS` in `perf_counters.rs` is 128 and a const
   assert keeps it above the highest opcode.

## Consequences

- `arith` drops from 43 to 24 counted ops per iteration (the `intarith` round
  body 132 → 88), gated by `tests/perf_counters_env_hashing.rs` and a compile-time
  shape test in `bytecode/tests.rs`.
- The trailing-return decision in `Compiler::finish` reads the last emitted
  *opcode* (`last_op_at`), not the last byte: a fused store now ends the chunk on
  an operand byte whose value can equal `Return`.
- Not done here: a standalone small-integer immediate (no dispatch saved),
  Script-goal completion-value elision, loop inversion (test at the bottom),
  frame-register locals, a local-local binary form.
