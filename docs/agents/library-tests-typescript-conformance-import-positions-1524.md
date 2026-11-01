# Library Tests: the position of a dynamic `import()` under `--node` (#1524 item 3)

Part of epic #1214 (tracker #1418). `--node` only. Follows
[library-tests-typescript-conformance-module-errors-1484.md](library-tests-typescript-conformance-module-errors-1484.md),
which derived the rules and left them unimplemented.

## What changed

`Expression::Import` carried the offset of the `import` keyword (or of its specifier's last variable load), so an
`import()` nested in other expressions reported a different column than Node (`id(import(x))`, `a && import(x)`,
`(await import(x))`, `() => import(x).then(f)`, `Promise.all([import(x)])`). The position is now computed while parsing,
the way V8's bytecode generator emits positions (`src/parser/v8_position.rs`):

- `Track { last, pending }` per thread, touched only once the stack hooks are installed (`inferred_names::enabled()`).
- **Statement position** (`statement`): the start of every statement, of each declarator's initializer, of a `for`
  initializer, `last = start; pending = true`.
- **Expression position** (`expression`): `if pending { pending = false } else { last = position }`. The position
  is dropped while a statement position is pending, and the instruction it belongs to ends the pending state. Events:
  a variable load (identifier start), a property load (the name; the `.` after a call; the `[` after its key; the `.`
  of a private name; `?.` for an optional one), a call (callee name or `(`, after the arguments), `new` (after the
  arguments), a binary operator (after the right operand), an assignment's `=` (after the right side), a postfix `++`,
  an object property value (its start, not for constants of a boilerplate literal), a spread argument (after the
  operand), `new.target`.
- **Early expression position** (`early_expression`): `await` and the backtick of a tagged template; dropped while
  pending, and leaves it pending. `await` records its position again once the operand is evaluated (`id(await n, import(x))`
  reports the `await`, not the load of `n`).
- **Instruction without a position** (`instruction`): a literal, `this`, an array or object literal, a function,
  `import` itself (it emits a bytecode before it reads its specifier, so `import(v)` reports the load of `v`), a
  template head with text, a binding pattern; only ends the pending state. An array or object literal whose first
  element is a spread, a parenthesis and an untemplated empty template head do not emit one.
- Stores are not reads: `w = v` forgets the identifier `w`, `o.p = v` forgets the property load of `p`, a binding
  pattern forgets its names (`store_target`). `typeof name` does not load with a position.
- A function (declaration, expression, method, arrow, class static block, field initializer) starts with an empty
  state, including its parameters (`enter_function`, in `with_function_context` and `parse_formal_parameters`). An
  arrow's parameter list is parsed first as an expression and its events are undone when `=>` follows
  (`track_primary_end`).
- **Statement positions that are a node's position.** A `while`/`do`/`for` condition, a `for` update, a `case` test, a
  `for-in/of` subject, the right operand of a comma and the concise body of an arrow take the position V8 gives that
  *node* (`a && b`: the `&&`; `a ?? b`: the `b`; `a ? b : c`: the `a`; `f(x)`: `f`; `a.b`: `b`; `await x`: the
  `await`; `i = x`: the `=`), which is known after the node is parsed. `begin_node`/`end_node` set a marker as the
  statement position; an `import()` that reads the marker gets an index into a table (`SLOT`, resolved at run time by
  `v8_position::resolve` in `eval.rs`, only when it reads one), filled when the node is complete. The `NODE` register
  holds the position of the last completed node (`expression`, `node`).
- The tree-walker stores the position in `call_pos` after it has evaluated the specifier and the options (a call in
  the specifier does not change what V8 reports for the `import` that follows it). `import()` is a tree-walker
  construct in the bytecode VM (the script poison), so both engines read the same field.
- `retarget_import`/`positions_itself` (the former approximation) are gone.

Files: `src/parser/v8_position.rs` (state and helpers), hooks in `expressions.rs`, `expressions/{binary_ops,
logical_ops,new_expr,optional_chain,arguments}.rs`, `statements.rs`, `declarations.rs`, `declarations/{declarator_list,
patterns}.rs`, `cover_init.rs`, `positions/for_head_site.rs`. The size budget moved three blocks out to make room:
the optional chain arm of `parse_left_hand_side_expression` (`expressions/optional_chain.rs`), `??`/`&&`/`||`
(`expressions/logical_ops.rs`) and `ParseError` (`parse_error.rs`).

## Verification

`tests/node-import-positions-1524/shapes.txt` holds 332 module bodies with the position Node v26.8.2 reports
(`LINE:COLUMN`); `scripts/run-node-import-positions-1524-oracle.py [--node]` runs them on jsse (and Node) and
`--node` rewrites the expectations. The shapes were found by probing, then re-probed blind: the first 110 shapes came
from the rules in the 1484 document (87 differed before the change), the next 148 and 83 were written afterwards to find
what the rules missed, and the remaining differences were fixed or listed below. `tests/node_import_positions_1524.rs`
replays all of them on the VM, the tree-walker and under `JSSE_GC_STRESS=7`, and parses a sample in default mode.
`scripts/node-stack-fixtures-1482/import-site.mjs` and the other stack fixtures still pass.

## Not met

Shapes probed on Node that jsse still reports differently (none is in `shapes.txt`):

- An `import()` in the default of an arrow parameter, `(a = id(import(x))) => a`: the list is parsed as an expression
  in the enclosing function's state; V8 compiles it in the arrow, so it reports the load of `id` and jsse reports the
  enclosing statement.
- A class heritage, a computed class key and a decorator argument: not probed.
- A Node `catch`-style case (`import(x).catch(f)` never rejects unhandled) and `new import(x).constructor` are not
  observable through the harness; they are not part of the issue.
