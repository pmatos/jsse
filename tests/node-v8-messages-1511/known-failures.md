# Known failures: V8 error positions under `--node` (#1511, stages 1 to 4)

Stage 1 gave binary operators and class heritage their V8 position
(`operators.js`); stage 2 gave constant assignment and update, the failed read
of a nullish compound/update/`delete` target, `new` callee chains,
optional-chain members, parenthesised and lowered heritage, and an array
assignment pattern their position (`assign.js`, `heritage.js`). Stage 3
(`patterns.js`, `params.js`, [its notes](../../docs/agents/library-tests-typescript-conformance-v8-messages-1511-stage3.md))
gave destructuring declarations and assignments, array declarations, for-of
operands, array and call spreads and destructured parameters their position
and callee frame. Stage 4 (`statements.js`, `arrays.js`,
[its notes](../../docs/agents/library-tests-typescript-conformance-v8-messages-1511-stage4.md))
gave statements and identifier reads their position: a missing or
uninitialised binding, `delete null.x`, a nullish compound target, a symbol
operand, `let z = z`, `s += sym`, a comma heritage, array pattern parameters
and nested patterns, `Math` as a heritage and a lone spread argument.

What is left reports the frame's last position (or the function start) where
V8 reports the column given. Columns are Node 26.8, `show()` from the fixture,
one statement per line indented by two spaces.

| Case | V8 reports | Why it differs |
| --- | --- | --- |
| `class D extends 1 { [yield 2]() {} }` in a generator, heritage rejected after the key was resumed | the computed key | a V8 quirk of resumed generators |
| `class B extends (() => {}) {}` | the end of the arrow body (col 28 on a one-line statement), jsse the heritage | a V8 quirk for a parenthesised arrow heritage |
| `1 && x`, `true ? x : 1`, `0 ? 1 : z` | the statement start | V8 folds the constant test away, so the identifier becomes the first bytecode of the statement; jsse reports the identifier |
| `with (x) {}`, `x` missing | the statement start (col 3) | `Statement::With` carries no offset (33 pattern sites for a sloppy-mode-only statement); jsse reports the identifier |
| `1, x;` | the operand (col 6) | V8 positions each operand of a comma expression; jsse the statement start |
| `function ([a], [b]) {}`, `function ({ a }, [b]) {}`, `function ([a] = undefined) {}` called with `undefined` | the earlier parameter's last position / the default's | V8 reports the last position emitted by the parameters before; jsse the `(` |
| `[[a]] = [null]`, `try { throw 1 } catch ([a]) {}` | the inner `[` / the `throw` | the cover grammar's patterns carry no offset; the catch parameter is bound at the stale statement |
| `a + z` or `a += z` with `a` and `z` both frame locals, `z` uninitialised | the identifier `z` (col 7) | the fused local-local op reads both operands and has no identifier table; jsse reports the statement start. Reading a global or an outer binding (`s += sym`) and the tree-walker match |
| `for (x of 5)` and `f(1, ...5)` in a generator or async function under `--no-bytecode` | the operand; `5 is not iterable` wording for a for-of | the state-machine terminator `ForOfInit` and the lowered call spread carry no offset and word the error as an array spread. The VM, the default, matches |

The `1 && x` / `0 ? 1 : z` rows are a consequence of V8 folding a constant
test away; the two "V8 quirk" rows are behaviours of V8 that no rule predicts.
