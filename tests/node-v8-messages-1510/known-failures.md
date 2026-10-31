# Known failures: V8 messages under `--node` (#1510, #1511, #1512)

Differences from Node 26.8 that the fixtures here do not cover. Fixtures
compare stdout of `node` and `jsse --node`; `callees.js` prints the top frame
unless a case is in its `messageOnly` set.

| Case | Difference | Issue |
| --- | --- | --- |
| frames of builtin errors (`at Array.map (<anonymous>)`) | jsse has no frame for a native function | #1482 (#1525) |
| private brand checks (`o.#m()` on a foreign object, `#x in 1`), `Promise.prototype.finally.call(1, f)`, symbol-keyed method receivers, `Array.prototype.with` range text | see `tests/node-v8-messages-1532/known-failures.md` | #1532 |
| `(function (a = b, b) {})()` with an outer `b` | engine reads the outer binding instead of the TDZ error | #1533 |
| position of `destructure*`, `array*`, `forOf*`, `spread*` cases in `callees.js` (the `messageOnly` set) | caret at the statement or call, V8 at the property, initializer or operand | #1511 |
| operator, identifier, TDZ, const-assignment, heritage and compound-assignment positions | see `tests/node-v8-messages-1419/known-failures.md` | #1511 |
| `for (const [a] of [5])` and an array pattern nested in an object pattern | V8 prints `.for is not iterable` and `{(intermediate value)}` | #1512 |
