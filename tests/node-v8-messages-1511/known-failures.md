# Known failures: V8 error positions under `--node` (#1511, stages 1 to 3)

Stage 1 gave binary operators and class heritage their V8 position
(`operators.js`); stage 2 gave constant assignment and update, the failed read
of a nullish compound/update/`delete` target, `new` callee chains,
optional-chain members, parenthesised and lowered heritage, and an array
assignment pattern their position (`assign.js`, `heritage.js`). Stage 3
(`patterns.js`, `params.js`, [its notes](../../docs/agents/library-tests-typescript-conformance-v8-messages-1511-stage3.md))
gave destructuring declarations and assignments, array declarations, for-of
operands, array and call spreads and destructured parameters their position
and callee frame. Everything below still reports the frame's last call position (or the function start)
where V8 reports the column given. Columns are Node 26.8, `show()` from the
fixture, one statement per line indented by two spaces.

| Case | V8 reports | Needs |
| --- | --- | --- |
| `x is not defined` as an operand (`return x + 1`) | the statement start (col 3): V8 has no position of its own for a global load | a statement offset on the frame |
| `x()` with `x` undeclared | the identifier, which is the statement start (col 3) | the same |
| `delete null.x` | the statement start (col 3): `delete` has no position of its own | the same |
| `n.y += 1`, `m[0]--`, `delete m.y` on a nullish identifier with no member or call in the operands | the statement start (col 3); jsse reports the access's own position, same line | the same |
| `let s = sym; s += 1` and `s += 1n` (the left operand of a compound assignment fails to convert) | the statement start (col 3) | the same |
| `s += sym` with `sym` a top-level `const`/`let` | the `sym` operand (col 8): V8 emits a position for a load that needs a TDZ hole check, and for no other variable load | an offset on `Expression::Identifier` (177 pattern sites, 83 in `generator_transform.rs`) |
| `let z = z` (TDZ) | the initializer identifier (col 11), the same hole-check position | the same |
| `class B extends (1, num) {}` | the last operand of the comma expression (col 23); jsse the first (col 20) | an offset per sequence element |
| `class D extends 1 { [yield 2]() {} }` in a generator, heritage rejected after the key was resumed | the computed key (a V8 quirk of resumed generators) | not worth matching |
| `class B extends (() => {}) {}` | col 37, past the end of the line: a V8 quirk for a parenthesised arrow heritage | not worth matching |
| `Class extends value #<Object>` for `Math` | jsse prints `#<Math>`, V8 `#<Object>` | V8 uses the constructor name, not `@@toStringTag` |
| `function ([a]) {}` called with `undefined` | the `(` of the parameter list (col 13); jsse the function start | an offset on `Pattern::Array` or on the parameter list |
| `f(...5)` and `new f(...5)` (a sole spread argument) under `--no-bytecode` | the call (col 3), as in the VM | the tree-walker's `eval_spread_args` does not know the call's position |
| `for (x of 5)` or `[...5]` in a generator or async function that suspends | the stale position of the frame | the state-machine terminators (`ForOfInit`) and the lowered `Spread` carry no offset |
| `async function (x = 1, { a } = null) {}` | V8 words it `Cannot destructure property 'a' of '(intermediate value)...'` | message text, not a position |
| `var { s: [t] } = { s: null }` | the `[` (col 12) | an offset on `Pattern::Array` (message: #1545) |

## Design notes for the next stages

- Statement starts: a `u32` on `Statement::Expression`, `Return`, `Variable`
  and the control statements is about 200 pattern sites. A marker statement
  in the lists was rejected: directive prologues, labelled and unbraced
  bodies, body-shape checks, hoist caches, the generator lowering and the
  per-statement safepoint (which would double, shifting `JSSE_GC_STRESS`
  schedules) all depend on the list shape.
- The oversized files may not grow, so each edit moves code out first (stage
  2 moved the test modules of `ast.rs`/`eval.rs`/`parser/mod.rs`, the
  prop ops of `vm.rs`, the new-expression parser of `expressions.rs`, the
  class-hoisting pass of `generator_transform.rs` and others).
