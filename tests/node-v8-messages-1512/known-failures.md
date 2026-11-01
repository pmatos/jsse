# Known failures: V8 messages under `--node` (#1512, #1532)

Differences from Node 26.8 that the fixtures here do not cover. Fixtures
compare stdout of `node` and `jsse --node`. The rows are tracked in #1557
(destructuring and iteration) and #1558 (builtins), both sub-issues of #1418.

## Destructuring and iteration (#1557)

| Case | V8 | jsse |
| --- | --- | --- |
| `for (x of o.m())`, `[...o.m()]`, `var [a] = o.m()` with `o.m` not callable | `o.m is not a function or its return value is not iterable` | `o.m is not a function` |
| `` var [a] = five`t` `` with `five` not callable | `five is not a function or its return value is not iterable` | `five is not a function` |
| `var [a] = (nul)` | `object null is not iterable (cannot read property Symbol(Symbol.iterator))` | `nul is not iterable` |
| `var [a] = [] = nul` | `[] is not iterable` | `object null is not iterable (...)` |
| `for (x of 1 + 1)` | `2 is not iterable` | `(1 + 1) is not iterable` |
| `(function ({ a } = nul) {})()` | `Cannot destructure property 'a' of '(intermediate value)(intermediate value)(intermediate value)' as it is null.` | `Cannot read properties of null (reading 'a')` |
| `(function ({ a }, [b]) {})({})` | `Cannot destructure property 'Symbol(Symbol.iterator)' of 'undefined' as it is undefined.` | `undefined is not iterable (cannot read property Symbol(Symbol.iterator))` |
| uncaught destructuring error in an async function's parameters | position of the pattern | position of the function |
| `a["]"]()` with `a["]"]` not callable (the callee scanner stops at the `]` inside the string) | `a.] is not a function` | `1 is not a function` |

## Builtins (#1558)

| Case | V8 |
| --- | --- |
| `Promise.prototype.finally.call({ then: 1 }, f)` | `number 1 is not a function` |
| `new SharedArrayBuffer(1).grow(1)` | `Method SharedArrayBuffer.prototype.grow called on incompatible receiver #<SharedArrayBuffer>` |
| Proxy `getOwnPropertyDescriptor`, `setPrototypeOf`, `isExtensible`, `apply` and `construct` trap results | property and value text, see #1558 |
| private field initialized twice; `Object.prototype.__proto__ = {}` | `Cannot initialize #y twice on the same object`; `Immutable prototype object 'Object.prototype' cannot have their prototype set` |
| `[][Symbol.iterator]().next.call(1)`; `Iterator.from(5)` | `Method Array Iterator.prototype.next called on incompatible receiver 1`; `Iterator.from called on non-object` |
| `Intl.NumberFormat` option range text; `Intl.Collator.prototype.compare` getter receiver | see #1558 |
| `FinalizationRegistry.register(1)`; `structuredClone()`; `new RegExp('\\', 'u')` | `... invalid target`; Node's `ERR_MISSING_ARGS`; flags after the pattern in the syntax error |

## Rows of `tests/node-v8-messages-1532/known-failures-1544-1545.md`

Fixed here: the anonymous class receiver (`Receiver must be class .` is Node
26.8's text for a static private method, jsse matches it), `({ a: [b] } =
five)`, `try {} catch ({ a }) {}` with null (`.catch`), and `[1].with(i, v)`
(Node 26.8 reports no error for an object index with a `toString`). Still open:
`Promise.prototype.finally.call({ then: 1 }, f)` (#1558).
