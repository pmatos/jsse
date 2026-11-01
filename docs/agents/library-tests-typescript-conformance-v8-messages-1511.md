# V8 error positions of operators and class heritage (#1511, stage 1)

Epic #1214. Continues `library-tests-typescript-conformance-v8-messages-1419.md`
(member accesses and calls got their position there). Everything here is
`--node` only: the parser records an offset only once the stack hooks are
installed (`parser::inferred_names::enabled()`), so default mode parses,
compiles and runs exactly as before.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1511-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the `.out`).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1511`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, and the default-mode wording).
- Fixture: `tests/node-v8-messages-1511/operators.js` with Node's output in
  `operators.out`; what is still different, with V8's columns, in
  `tests/node-v8-messages-1511/known-failures.md`.
- Unit tests: `bytecode/tests_binary_pos.rs` (empty table without hooks, the
  recorded offsets, `size_of::<Expression>() == 112`) and
  `builtins/node_host_heritage.rs`.

## What shipped

- `Expression::Binary(op, left, right, u32)`: the fourth field is the operator
  token's offset plus one under `--node`, else 0. It sits in the variant's
  padding, so `Expression` stays 112 bytes. (An offset on `ExprBox` was
  measured against: it would make every boxed node 120 bytes, the next
  allocator class up.) The productions from `|` down to `*` moved to
  `parser/expressions/binary_ops.rs` and record it (`op_pos`).
- Tree-walker: `eval/binary_expr.rs` (the old `Expression::Binary` arm) sets
  the frame's position to it before the operator runs (only `--node` parses a
  nonzero offset, so default mode pays one compare). A throw
  from the operator, or from a `valueOf` it calls, then reports V8's column,
  and so does the caller's frame in the stack.
- VM: `compiler/binary.rs` records `Chunk::binary_pos`, keyed by the offset
  *after* the instruction (a fused `BinConst`/`LocalBinConst`/`LocalLocalBin`
  or a compare-jump included), because that is the `pc` every slow path has
  already advanced to. `vm/binary_slow.rs` (the non-Number paths, moved out of
  `vm.rs`) and the `In`/`Instanceof` arm call `member_pos::note_binary`, which
  returns on an empty table. The Number fast paths are untouched.
- Class heritage: no offset on the AST. `builtins/node_host_heritage.rs` reads
  the class's own source text (`class`, an optional name, `extends`, skipping
  comments) and sets the position to the heritage expression before
  `class_extends_error` makes the TypeError. A heritage the generator lowering
  already checked (`ClassProtoParent`) still reports the frame's old position.
- `Symbol() + 1` reads `Cannot convert a Symbol value to a number` under
  `--node` (a string operand keeps `... to a string`): `node_host_symbol_add.rs`.
- The generator/async lowering keeps the offset of the operators it rebuilds
  (`generator_transform.rs`); operators it synthesises carry 0.

## Why the files moved

`ast.rs`, `eval.rs`, `compiler.rs`, `vm.rs`, `parser/expressions.rs` and
`generator_transform.rs` are over 20,000 bytes and may not grow. Each change
moved at least as much code out into a sibling file (`ast/expr_box.rs`,
`eval/binary_expr.rs`, `compiler/binary.rs`, `vm/binary_slow.rs`,
`expressions/binary_ops.rs`, `generator_transform/undefined_check.rs`) and a
few comments were tightened where a pattern gained `, _`.

## Not done (stop rule)

Assignment, update, identifier, TDZ, compound-member, `delete`, `new`-chain and
optional-chain positions: see the table and the design notes in
`tests/node-v8-messages-1511/known-failures.md`. They need the same kind of
field on `Expression::Assign` and `Update` (76 pattern sites) and a statement
position for identifier errors; #1511 stays open for them. test262 cannot
observe any of this (default mode is unchanged), so the regression coverage is
the Node oracle above rather than `test262-extra/`.

Stage 2 (assignment, update, nullish compound targets, `new` chains, optional
chains, lowered heritage): [library-tests-typescript-conformance-v8-messages-1511-stage2.md](library-tests-typescript-conformance-v8-messages-1511-stage2.md).
