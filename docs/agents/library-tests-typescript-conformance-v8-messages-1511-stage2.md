# V8 error positions of assignments, updates and heritage (#1511, stage 2)

Epic #1214. Continues
[stage 1](library-tests-typescript-conformance-v8-messages-1511.md) (binary
operators and class heritage). Everything here is `--node` only: the parser
records an offset only once the stack hooks are installed
(`parser::inferred_names::enabled()`), so default mode parses, compiles and
runs exactly as before.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1511-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the `.out`).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1511`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, default-mode wording).
- Fixtures: `tests/node-v8-messages-1511/assign.js` and `heritage.js` with
  Node's output beside them; what is still different, with V8's columns, in
  `tests/node-v8-messages-1511/known-failures.md`.
- Unit tests: `bytecode/tests_store_pos.rs` (the table is empty without hooks),
  `builtins/node_host_heritage.rs` (parenthesised heritage).

## What V8 reports (Node 26.8)

V8's position is the last one its bytecode emitted before the failing
instruction, so each row has its own rule:

| Error | Position |
| --- | --- |
| constant assignment `c = 2`, `c += 2`, `c &&= 2` | the operator token (`=`, `+=`, `&&=`) |
| constant `c++` / `++c` | the `++` of the postfix form, the operand of the prefix form |
| nullish base of `o.x.y += 1`, `o.x.y++`, `delete o.x.y` | the last position the operands emitted: the key's if it has one (`o.x[o.k]` reports `.k`), else the base's (`.x`) |
| `new o.a.b.c()` with `o.a` undefined | the failing member's own name (`.b`), as for a plain read |
| `o?.a.b` with `o.a` undefined | the `.` before `b` (not the name, unlike a plain read) |
| `class B extends (num) {}` | inside the parentheses |

## What shipped

- `Expression::Assign(op, l, r, u32)` and `Expression::Update(op, prefix, e,
  u32)`: the last field is the `=`/`++` offset plus one under `--node`, else 0.
  It sits in padding, so `Expression` stays 112 bytes (unit test in
  `tests_binary_pos.rs`). The parser takes it from the operator token
  (`op_pos`); the generator/async lowering copies it onto the nodes it
  rebuilds and gives synthesised stores 0.
- Constant assignment: `const_assign_error` takes `NodeHost::assign_pos` and
  makes it the frame's position. The tree-walker sets it only around the
  store (`put_ref_at`, `env_set_at`, `update_identifier_at` in
  `builtins/node_host_assign_pos.rs`), so the right-hand side cannot see a
  stale one. The VM reuses `Chunk::binary_pos` keyed by the offset after the
  store (`compiler/ident_store.rs`, which holds `compile_identifier_assign` and
  `compile_update`, moved out of `compiler.rs`); `StoreLocal`, `StoreResolvedName`
  and the update ops read it in their by-name slow path only
  (`vm/name_store.rs`, `member_pos::note_store`), so the hinted fast paths are
  untouched.
- Compound/update/`delete` on a nullish base: `last_member_pos` picks the key's
  last position, else the base's, else the access's own. The VM compiles the
  reading op of a compound member assignment, `UpdateProp`, `UpdateElement`
  and `ToPrimitiveKey` with that position as their `member_pos` site (they
  were not recorded before); the tree-walker calls `note_last_pos`.
- `new` callee chains and optional-chain members take `PropSiteId::at(..)`
  positions in the parser instead of `UNASSIGNED` (the IC numbering pass
  keeps the position). An optional-chain tail reading a nullish value now
  evaluates a computed key before throwing, so the message names the key
  (`reading 'k'`, was `'property'`) as the specification's order requires.
- Heritage: `heritage_offset` skips the parentheses around the expression,
  and `Expression::ClassProtoParent(heritage, u32)` carries the position the
  generator lowering computed from the class source text
  (`node_host_heritage::heritage_pos`), so a heritage rejected after the
  suspension of `class B extends (yield) {}` or `(await x)` reports it too.
- An array assignment pattern (`[a] = n`) sets the frame's position to the
  `=`, as V8 does (`arrayAssign` left `messageOnly` in 1510's `callees.js`).

## Why the files moved

`ast.rs`, `eval.rs`, `vm.rs`, `compiler.rs`, `generator_transform.rs`,
`generator_analysis.rs`, `parser/expressions.rs`, `parser/mod.rs` and others
are over 20,000 bytes and may not grow, and 76 `Assign`/`Update` pattern sites
gained a field. Compensation: the test modules of `ast.rs`, `eval.rs` and
`parser/mod.rs` moved to sibling files; the prop ops of `vm.rs`
(`vm/prop_ops.rs`), the new-expression parser (`expressions/new_expr.rs`), the
class-hoisting pass (`generator_transform/class_hoist.rs`), the optional-chain
delete (`eval/access/optional_delete.rs`), the class heritage checks
(`eval/literals/class_heritage.rs`) and a head scan
(`generator_analysis/head_await.rs`) moved verbatim; patterns were shortened to
`Update(.., e, _)`.

## Not done (stop rule)

Statement starts (undeclared `x`, `delete null.x`, the left operand of a
compound assignment), identifier positions (TDZ, `s += sym` on a top-level
`const`), destructuring, for-of and spread operand positions: see the table
and the design notes in `tests/node-v8-messages-1511/known-failures.md`. Each
needs an offset on a node with 50 to 200 pattern sites (or a statement offset,
since a marker statement was rejected as it changes every list-shape
dependency), and #1511 stays open for them. test262 cannot observe any of
this (default mode is unchanged), so the regression coverage is the Node
oracle above rather than `test262-extra/`.
