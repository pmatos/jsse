# V8 error positions of statements and identifiers (#1511, stage 4)

Epic #1214. Continues
[stage 3](library-tests-typescript-conformance-v8-messages-1511-stage3.md)
(patterns, for-of, spread, parameters). Everything here is `--node` only: the
parser records an offset only once the stack hooks are installed
(`parser::inferred_names::enabled()`), so default mode parses, compiles and
runs exactly as before. `Expression` stays 112 bytes and `Statement` within its
budget; a default chunk has no `SetCallPos` op and an empty position table
(`bytecode/tests_stmt_pos.rs`).

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1511-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the `.out`).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1511`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, default-mode wording).
- Fixtures: `tests/node-v8-messages-1511/statements.js` (statement starts and
  identifier reads, 48 cases) and `arrays.js` (array patterns, parameter lists,
  comma heritage, lone spread), Node's output beside them. What is still
  different is in `tests/node-v8-messages-1511/known-failures.md`.

## What V8 reports (Node 26.8)

V8 puts the statement's position on the first bytecode of a statement and a
position of its own on a few expressions; an error without one is reported at
the latest position emitted before it. The rules the fixtures pin down:

| Error | Position |
| --- | --- |
| a missing or uninitialised binding read first in an expression, `return`, `throw`, `if` or `switch` statement (`return x + 1`, `if (z)`, `x.p`, `-x`) | the statement's start (`return`, `if`, ...) |
| the same read anywhere else (`1 + z`, `f(1, z)`, `o.p = z`, `[0, z]`, `let z = z`, `var a = 1 + x`) | the identifier |
| a callee that is missing (`x()`, `new x()`, `1 + x()`) | no position of its own: the statement's start |
| `x++` / `x = 1` (strict) on a missing binding | the statement's start for the read of `x++`; the `=` for the store |
| `delete null.x`, `n.y += 1`, `n[0]--` (a nullish identifier or literal base) | the statement's start |
| `s += 1` / `s += 1n` with `s` a symbol, `+sym`, `` `${sym}` `` | the statement's start |
| `s += sym`, `s += f()` ... (the operator of a compound assignment fails) | the last position the right-hand side emitted: the identifier `sym`, the call, ... |
| a concise arrow body `() => x + 1` | the expression's own position (the operator of a binary one), a bare identifier at itself |
| `extends (1, num)` | the last operand of the comma expression |
| an array pattern parameter, called with `undefined` | the `(` of the parameter list (an arrow's too); any other array pattern its `[`; a declaration's own the initializer |
| `Class extends value #<Object>` for `Math` | V8 names an object by its constructor, not by `@@toStringTag` |
| `f(...5)`, `new f(...5)` | the call |

## What shipped

- `Statement::Expression(expr, at)` and `Statement::Return(expr, at)` carry the
  statement's start plus one; `IfStatement::at` and `SwitchStatement::at` the
  keyword; `Throw` already had one. The tree-walker calls
  `node_note_statement_pos` on entry (a no-op when 0); the compiler emits one
  `SetCallPos` per such statement (`emit_leading_pos`) only while the hooks are
  on, so a `--node` chunk pays one small op per statement and a default chunk
  none. The generator lowering keeps the position: a statement split at a
  suspension is led by a `null;` statement carrying it (`pos_marker.rs`), and
  `rewrite_stmt_await_to_yield` copies it.
- `Expression::Identifier(name, at)`: the identifier's offset plus one. The
  parser clears it for the identifier a statement evaluates first
  (`clear_lead_ident`, an iterative walk of the leftmost spine; for `y = z` the
  value, for `o.p = z` the object), where V8's statement position wins over the
  hole check's. `Expression` did not grow: the offset fits the padding.
- A missing or uninitialised binding is reported at the identifier by moving
  the error afterwards, so no read path pays for it: `create_error` notes a
  `ReferenceError` for `x is not defined` / `Cannot access 'x' before
  initialization` (`NodeHostState::ident_error`, `node_host_ident_pos.rs`),
  and whoever evaluated the identifier moves its stack to the recorded offset
  (`node_place_ident_error`). The tree-walker does it in the `Identifier` arm
  when the resolution threw; the VM in `throw!`, which looks the op that threw
  up in `Chunk::member_pos` (a name load records its offset there when it is
  compiled, `compile_ident_load`). Callees are resolved without the arm, and a
  strict store to a missing binding takes the `=` (`assign_pos`).
- `s += sym`: `emit_compound_op` / `apply_compound_at` set the failing
  operator's position to the last one the right-hand side emitted
  (`compound_pos`: an identifier's own, else the member or call's).
- The failed read of a nullish base in a compound assignment, update or
  `delete` reports the key's last position, else the base's, else (new) the
  statement start the frame already has.
- `Pattern::Array(PatternElems)`, the newtype `PatternProps` introduced: `at`
  is the `[`, the parameter list's `(` for a formal parameter (also an arrow's,
  re-read by `reparse_as_formal_parameters`), and 0 for a declaration's own
  pattern. `bind_pattern` notes it before the iterator is taken.
- `sequence_tail` (`parser/source_scan.rs`) finds the last operand of a
  parenthesised comma heritage in the class source text, as stage 2 found the
  parenthesis.
- `v8_value_text` names an object by its default `toString` constructor, so
  `Math` is `#<Object>`.
- A lone spread argument takes the call's position in the tree-walker
  (`eval_spread_args` is given the call site).

## Why the files moved

Several of the files that gained a field are over 20,000 bytes and may not
grow. Constructions of `Expression::Identifier(x, 0)` became `ident(x)`
(`generator_transform.rs` shrank), and these moved verbatim into sibling files:
`ast/module_sites.rs`, `eval/compound_assign.rs`, `eval/access/member_key.rs`,
`exec/dispose_resources.rs`, `vm/frame_plain.rs`, `compiler/local_pair.rs`,
`generator_analysis/statements.rs`, `builtins/function_constructors/dynamic_parts.rs`,
`ic_store/mark_entered.rs`, `module_error_cache.rs`, `parser/eval_flags.rs`,
`parser/declarations/param_names.rs`, `parser/statements/do_while.rs`, and two
tests of `bytecode/tests.rs` to `tests_if_throw.rs`. The thirteen concise arrow
bodies of `parser/expressions.rs` now build their `return` in
`parser/lead_ident.rs`.

## Not done

See the table in `tests/node-v8-messages-1511/known-failures.md`: V8 quirks
(constant folding of `1 && x` / `0 ? 1 : z`, the resumed generator's computed
key, the arrow heritage past the end of the line), the `with (x)` statement
start, comma expression statements (`1, x;` reports each operand), a parameter
pattern that follows another parameter or has a default, an uninitialised
local read by a fused local-local op (no identifier table), and the
`--no-bytecode` generator state machine's for-of head. test262 cannot observe any of this
(default mode is unchanged), so the regression coverage is the Node oracle above
rather than `test262-extra/`.
