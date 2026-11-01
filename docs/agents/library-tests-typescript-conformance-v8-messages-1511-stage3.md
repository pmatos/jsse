# V8 error positions of patterns, for-of, spread and parameters (#1511, stage 3; #1566)

Epic #1214. Continues
[stage 2](library-tests-typescript-conformance-v8-messages-1511-stage2.md)
(assignments, updates, heritage). Everything here is `--node` only: the parser
records an offset only once the stack hooks are installed
(`parser::inferred_names::enabled()`), so default mode parses, compiles and
runs exactly as before.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1511-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the `.out`).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1511`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, default-mode wording).
- Fixtures: `tests/node-v8-messages-1511/patterns.js` (destructuring, array
  patterns, for-of, spread) and `params.js` (destructured parameters, with the
  callee frame), Node's output beside them. What is still different is in
  `tests/node-v8-messages-1511/known-failures.md`.

## What V8 reports (Node 26.8)

| Error | Position |
| --- | --- |
| `var { a } = null`, `({ a } = n)`, `function ({ a }) {}` called with `undefined`, a nested `{ s: { t } }` | the first property's value target (`a`; for `s: b` the `b`) |
| the same with `{}`, a computed first key | the pattern's `{` |
| the same with `{ ...r }` | the rest target `r` |
| `var [a] = n` | the start of the initializer (a member or call initializer too) |
| `for (x of y)` | the iterable's last recorded position: its start, or the property of a member (`o.p`) / the call of `f()` |
| `[...y]`, `f(1, ...y)` | the same, for the spread operand |
| `f(...y)` (the only argument) | the call |
| a destructured parameter | the same position as a declaration, and a frame for the callee as well as the caller |

## What shipped

- `Pattern::Object(PatternProps)`: the properties derefs to the `Vec`, so the
  readers did not change; `at` is the offset plus one of the first target
  (parser `parse_object_pattern`). Patterns the cover grammar converts
  (`expr_to_pattern`) and the generator lowering synthesises carry 0, except
  arrow parameters, which `arrow_params.rs` copies from the refined list
  (`adopt_positions`). An object literal assignment target is read back from
  the source text (`parser/expressions/pattern_pos.rs`), because
  `Expression::Object` has no room for an offset; the answer is the `Assign`'s
  `at`, which an object target never used.
- `VariableDeclarator::init_at` is now recorded for every declarator, so an
  array pattern reports the initializer. The tree-walker sets the position at
  `bind_declarator`/`require_destructurable` (the object cases overwrite it
  with the pattern's); the VM's `let`/`const` object patterns pass it to
  `emit_member_at` for their first `GetProp` (`Chunk::member_pos`, read only
  when the base is nullish).
- `ForOfStatement::right_at`, `Expression::Spread(operand, at)`: the start of
  the operand (`tail_pos` replaces it with the member's or call's own position).
  `Spread` stays in the 112 bytes of `Expression`. The VM records it for
  `GetIterator`, `ForInHead`, `ArgSpreadFirst` and `ArgSpread` in
  `Chunk::member_pos` (`emit_member_at`); `vm/member_pos.rs::note` and
  `vm/spread.rs` read it before the op runs, and an empty table costs the
  emptiness check they already made. The tree-walker calls
  `node_note_statement_pos`.
- Parameters: the non-simple parameter list is bound inside
  `node_in_async_frame` (the frame async functions already had), on the
  general call path and the compiled path with a named-parameter plan. Without
  `--node` it is one `installed` test per such call.
- The array-literal spread of a function the generator lowering rewrites keeps
  its position (`lower_spread`).

## Why the files moved

Several of the files that gained a field or a pattern are over 20,000 bytes and
may not grow. The methods moved verbatim into sibling files:
`eval/spread_args.rs`, `statements/try_statement.rs`,
`compiler/array_literal.rs`, `exec/eval_body.rs`,
`expressions/template_literal.rs`, `parser/strict_patterns.rs`,
`eval/literals/property_key.rs`, `vm/equality_op.rs`,
`generator_transform/stmt_queries.rs`; the test module of `ast.rs` moved to
`ast/bound_names_tests.rs`; the scanning helpers of
`builtins/node_host_heritage.rs` to `parser/source_scan.rs`, which the parser
and the heritage reader share.

## Not done

Statement starts (undeclared `x`, `delete null.x`, the left operand of a
compound assignment), identifier positions (`s += sym`, `let z = z`), the
comma-expression heritage, the `(` V8 reports for an array pattern parameter, a
sole spread argument under `--no-bytecode`, and for-of / spread inside the
state-machine paths: see the table in `known-failures.md`. The first three
need an offset on a node with 177 (`Identifier`) or about 200 (statement)
pattern sites, so #1511 stays open for them. test262 cannot observe any of
this (default mode is unchanged), so the regression coverage is the Node
oracle above rather than `test262-extra/`.
