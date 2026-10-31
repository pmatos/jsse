# Known failures: V8 messages and positions under `--node` (#1419)

Differences from Node 26.8 that the fixtures here do not cover, each tracked as
a sub-issue of #1418. `messages.js` prints only the message (no frame) for the
cases marked "position". The message text of builtin errors, destructuring,
iteration and non-name-chain callees is covered by `tests/node-v8-messages-1510`
(see its `known-failures.md`).

| Case | Difference | Issue |
| --- | --- | --- |
| `'abc'.repeat(-1)`, `[1].map(5)`, `new Promise(1)`, `Reflect.ownKeys(1)`, `JSON.parse('{')`, `Object.setPrototypeOf(1)`, `delete null.x` | the text matches; Node adds `at Array.map (<anonymous>)` style frames | #1482 (#1525) |
| `1 in 1`, `x instanceof 1`, `class B extends 1 {}` (position) | caret at the statement, V8 at the operator / heritage | #1511 |
| `x_undefined_var`, `let z = z`, `const c = 1; c = 2`, `Symbol() + ''` | caret column 1 | #1511 |
| `o.x.y += 1`, `o.x[k] += 1` (position) | V8 reports the last evaluated sub-expression | #1511 |
| `var {a} = null`, `var [a] = null`, `for (const x of 5)`, `[...5]` (position) | the message matches; caret at the statement, V8 at the property, initializer or operand | #1511 |
