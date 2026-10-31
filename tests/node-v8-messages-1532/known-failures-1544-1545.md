# Known failures after #1544 and #1545 (V8 messages under `--node`)

Differences from Node 26.8 that remain in the areas of #1544 and #1545. The
fixtures `private-members.js`, `symbol-receivers.js` and `pattern-origin.js`
cover what matches.

| Case | Difference |
| --- | --- |
| an anonymous class that uses one of its own static private methods as a receiver (`class { #m() {} static #sm() {} #f; static a(o) { o.#sm(); o.#m(); } }`) | V8 prints `Receiver must be an instance of class .` for the instance member too; jsse prints `anonymous` |
| `({ a: [b] } = five)`, an array pattern nested in an object pattern of an assignment | V8 prints `five is not iterable (cannot read property Symbol(Symbol.iterator))`; jsse prints the value |
| `try {} catch ({ a }) {}` thrown null | V8 prints `Cannot destructure property 'a' of '.catch' as it is null.`; jsse prints the value |
| `[1].with(i, v)` with an object index that has a `toString` | V8 prints `Invalid index : [object Object]`, jsse prints `#<Object>` |
| `Promise.prototype.finally.call({ then: 1 }, f)` | V8 prints `number 1 is not a function`; jsse prints `1 is not a function` |
