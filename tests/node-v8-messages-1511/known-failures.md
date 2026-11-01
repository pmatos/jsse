# Known failures: V8 error positions under `--node` (#1511, stage 1 shipped)

Stage 1 gave binary operators (`in`, `instanceof`, arithmetic, relational,
BigInt) and class heritage their V8 position; `operators.js` covers them.
Everything below still reports the frame's last call position (or the function
start) where V8 reports the column given. Columns are Node 26.8, `show()` from
the fixture, one statement per line indented by two spaces.

| Case | V8 reports | Needs |
| --- | --- | --- |
| `c = 2` on a `const` | the `=` (col 5) | offset on `Expression::Assign` (the `=` token) |
| `c += 2` on a `const` | the `+=` (col 5) | the same |
| `c++` on a `const` | the `++` (col 4, postfix) | offset on `Expression::Update` |
| `++c` on a `const` | the operand `c` (col 5, prefix) | the same |
| `x is not defined` as an operand (`return x + 1`) | the statement start (col 3): V8 has no position of its own for a global load | a statement offset on the frame, or on `Statement::Return` and friends |
| `x()` with `x` undeclared | the identifier (col 3) | the same (here the statement start is the identifier) |
| `let z = z` (TDZ) | the initializer identifier (col 11) | offset on `Expression::Identifier` or its `ExprBox` |
| `o.x.y += 1` with `o.x` null | the `x` (col 5): the last sub-expression evaluated; jsse reports the `y` (col 7) | the member site of a compound target reads the base's own position |
| `o.x.y++` with `o.x` null | the `x` (col 5); jsse the function start | the same |
| `delete null.x` | the `delete` (col 3); jsse the function start | offset on `Expression::Delete` |
| `new o.a.b.c()` with `o.a` undefined | the `.b` (col 11); jsse the function start | the member site of a `new` callee is `PropSiteId::UNASSIGNED` |
| `o?.a.b` with `o.a` undefined | the `.b` (col 14); jsse the function start | optional-chain members carry no site (`PropSiteId::UNASSIGNED`) |
| `o += Symbol()` and the other compound assignments | the compound operator (`+=`) | the offset on `Expression::Assign`; today the frame keeps the last call position, which happens to be right only after a call on the same line |
| `class B extends (yield) {}` in a generator, heritage rejected after the resume | the heritage expression | `ClassProtoParent` carries no class source text (`class_proto_parent` passes `None`) |
| `class B extends (() => {}) {}` | col 37, past the end of the line: a V8 quirk for a parenthesised arrow heritage | not worth matching |
| `Class extends value #<Object>` for `Math` | jsse prints `#<Math>`, V8 `#<Object>` | V8 uses the constructor name, not `@@toStringTag` |

## Design notes for the next stages

- Each of the first rows needs a `u32` (offset plus one, 0 = none) on the
  variant, filled by the parser only when `inferred_names::enabled()` (the
  `--node` hooks), exactly like `Expression::Binary`'s operator offset. A
  variant has spare room: `size_of::<Expression>()` stays 112 bytes, whereas a
  position on `ExprBox` would grow every boxed node from the 112 to the 128
  allocator class.
- `Expression::Assign` (53 pattern sites) and `Update` (23) cost one edit per
  site; the oversized files may not grow, so move code out of each file first
  (as stage 1 did with `eval/binary_expr.rs`, `compiler/binary.rs`,
  `vm/binary_slow.rs`, `parser/expressions/binary_ops.rs`).
- The VM needs a table keyed like `Chunk::binary_pos`, read only in the cold
  path where the store op reports the const error (`const_assign_error`).
- Identifier errors want a statement offset rather than an identifier offset
  (`Expression::Identifier(String)` has 177 pattern sites, 83 of them in
  `generator_transform.rs`): the frame would carry the start of the statement
  that is running, as V8's latest emitted position does.
