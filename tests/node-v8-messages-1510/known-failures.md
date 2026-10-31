# Known failures: V8 messages under `--node` (#1510, #1511, #1512)

Differences from Node 26.8 that the fixtures here do not cover. Fixtures
compare stdout of `node` and `jsse --node`; `callees.js` prints the top frame
unless a case is in its `messageOnly` set.

| Case | Difference | Issue |
| --- | --- | --- |
| frames of builtin errors (`at Array.map (<anonymous>)`) | jsse has no frame for a native function | #1482 (#1525) |
| `String.prototype.trim.call(null)`, `Array.prototype.map.call(null, f)`, `RegExp.prototype.exec.call(1)`, `Function.prototype.call.call(1)`, `Promise.all.call(1)`, `BigInt.asIntN(-1, 1n)`, typed array and DataView range text, `Array.from` / spread of a non-callable `@@iterator`, Proxy trap and invariant text, `JSON.stringify` cycle path, private brand checks, `Object.defineProperties` on a frozen object, strict `undefined = 1`, `new ({ m() {} }).m()` | builtin error text | #1532 |
| `(function (a = b, b) {})()` with an outer `b` | engine reads the outer binding instead of the TDZ error | #1533 |
| position of `destructure*`, `array*`, `forOf*`, `spread*` cases in `callees.js` (the `messageOnly` set) | caret at the statement or call, V8 at the property, initializer or operand | #1511 |
| operator, identifier, TDZ, const-assignment, heritage and compound-assignment positions | see `tests/node-v8-messages-1419/known-failures.md` | #1511 |
| ``o.f`a` `` | tagged template records no call position: the frame is `1:1` and the callee text is not read back | #1512 |
| `f(...5)`, `f(...undefined)`, `var [[a]] = [5]` | call spread and nested array pattern wording | #1512 |
