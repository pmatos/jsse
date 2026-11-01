# Library Tests: keyword and statement positions in async frames (#1548)

Part of epic #1214 (tracker #1418). Closes the "Not matched" section of
`library-tests-typescript-conformance-generator-frames-1529.md`. `--node` only: default mode and both engines'
semantics are unchanged (every position below is 0 unless the Node prelude installed its hooks).

## What V8 reports

A frame parked where no expression has a position of its own (the `await` of a disposer, of an iterator's `return()`,
of a `yield*` delegation) is reported at the last position V8 recorded, in bytecode order. The engines record call,
`await` and operator offsets; this adds three more.

- **`yield` keyword.** `Expression::Yield` carries a `YieldKind { delegate, at }` (`ast/node_sites.rs`) instead of the
  `delegate` flag; `parse_yield_expression` (`parser/expressions/yield_expr.rs`) fills `at` with `op_pos()` and
  `StateTerminator::Yield` keeps it. It is used:
  - as the parked position of an async generator at a `yield*` delegation: `node_note_yield_pos` writes it to the
    generator's position slot when the terminator runs, `await_then_in_generator` no longer resets the slot, so every
    step of the delegation (`next`, `throw`, `return`) parks at the keyword, also when the operand is a call;
  - as the call position of a sync `yield*` whose operand is not a call (`delegate_site`; a call keeps its own
    position, as V8 does);
  - as the position an async generator keeps after a `yield`, so a disposal on the resumption that follows (`await
    using` in the generator, closed by `return()`) reports that `yield`.
  A `yield` the transform does not decompose (`Completion::Yield` out of a state body) has `at: 0` and leaves the slot.
- **Initializer of a declaration.** `VariableDeclarator.init_at` is the start of the initializer of an identifier
  binding (`op_pos()` after the `=`; a destructuring pattern leaves 0). After the initializer is evaluated the tree-walker
  (`exec/variable_declaration.rs`) and the compiler (`compiler/lexical.rs`: `emit_stmt_pos`, an `Op::SetCallPos`, no bytes
  without hooks) set the call position to it. V8 does the same for every initializer, a call or a folded `1 + 2`
  included, so `await using r = ...; const x = 1 + 2;` parks at the `1`, not at the `+` the binary expression recorded.
  The last declarator wins (`let q = 1, r = 2` is the `2`).
- **`break`.** `Statement::Break(label, at)` is the keyword. The tree-walker notes it when the statement runs
  (`node_note_statement_pos`); in a state machine a `break` is a `LoopControl` terminator, so `LoopControlTarget.at`
  carries it and `route_loop_control!` / `route_generator_loop_control` note it before the for-of loops it crosses are
  closed. A `for await` closed by `break` parks its `return()` `await` there. The compiler emits nothing for `break`:
  a `for await` is never compiled, and a `break` that leaves a chunk runs on the tree-walker.

The position is the call position of `node_host`, so the existing rule for a disposer's park (the latest call, else the
last `await`: `node_note_dispose_pos`) now sees these too.

## Verification

`scripts/v8-error-fixtures/generators-1529-gaps.js` (the four shapes the #1529 doc listed) and `positions-1548.js`
(26 frames: fifteen declaration and scope shapes, `yield*` of an async generator, of a sync generator and of an identifier,
disposal after a `yield` of a literal and of a call, and six ways to `break` out of a `for await`), with Node 26.8's
output pinned in `expected-generators-1529-gaps.txt` and `expected-positions-1548.txt` (regenerate with `node
scripts/v8-error-fixtures/<name>.js > .../expected-<name>.txt`). `tests/node_stack_frames_1548.rs` replays both under
the VM, the tree-walker and `JSSE_GC_STRESS=7`, and checks the same shapes in default mode. No `test262-extra/` test:
nothing here is observable through ECMAScript, only through the host's `Error.stack`.

## Not matched

Probed against Node 26.8 with `await using` as the last thing in a scope; the engines report the latest call,
declaration or `break` they executed, V8 the last position in source order.

- **Other statements.** `return` and `throw` (their keyword), an assignment (the `=`), `a++` (the `++`), an
  expression statement of a literal, a `for` statement (its keyword), `let y;` (the binding) and a destructuring
  declaration (its first element) are positions V8 records and the engines do not. A frame parked after one of them
  reports the previous call, declaration or `await`.
- **Static order.** Two `yield`s in a scope, closed after the first: V8 reports the second (the last in the bytecode),
  jsse the first.
- **A conditional `break`.** `for await (x of g()) { if (x) break; }` closes at the `for` keyword in V8 (the
  `break`'s position is lost; why is not modelled); `if (true) break;` is folded and keeps the `break`. jsse reports
  the `break` in both.
- **The caller of a running async generator.** `yield* asyncGen()` where the callee runs synchronously up to a
  `capture` before its first `await`: the delegating generator's live frame is at its function start; V8 reports the
  call.
