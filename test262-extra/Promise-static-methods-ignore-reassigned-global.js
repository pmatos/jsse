/*---
description: >
  The promise machinery uses %Promise%, not the value of the global binding:
  a subclass assigned to `Promise` is constructed by the inherited statics,
  and replacing the binding does not affect async functions or await.
esid: sec-promise-constructor
info: |
  Promise.resolve, Promise.reject and Promise.withResolvers perform
  NewPromiseCapability(this value), so with `this` a subclass the subclass
  constructor runs. Async functions and await use the intrinsic %Promise%
  whatever the global binding holds.
flags: [async]
---*/

var Orig = Promise;
var constructed = 0;
globalThis.Promise = class Sub extends Orig {
  constructor(executor) { constructed++; super(executor); }
};

var resolved = Promise.resolve(1);
var rejected = Promise.reject(1);
var capability = Promise.withResolvers();
assert.sameValue(resolved instanceof Promise, true, "resolve builds a subclass instance");
assert.sameValue(rejected instanceof Promise, true, "reject builds a subclass instance");
assert.sameValue(capability.promise instanceof Promise, true, "withResolvers builds a subclass instance");
assert.sameValue(constructed, 3, "the subclass constructor ran once per capability");

var derived = resolved.then(function () {});
assert.sameValue(derived instanceof Promise, true, "then derives through @@species");
assert.sameValue(constructed, 4, "then constructs through the subclass");

var result = (async function () {})();
assert.sameValue(Object.getPrototypeOf(result), Orig.prototype,
  "an async function's result promise is a %Promise%");
assert.sameValue(constructed, 4, "no subclass construction for async results");
rejected.catch(function () {});

globalThis.Promise = 1;
var order = [];
(async function () {
  await 1;
  order.push("await");
})();
Orig.resolve(2).then(function () { order.push("then"); });
(async function () { return 5; })().then(function (v) { order.push("result:" + v); });

var wait = Orig.resolve();
for (var i = 0; i < 8; i++) wait = wait.then(function () {});
wait.then(function () {
  globalThis.Promise = Orig;
  assert.compareArray(order, ["await", "then", "result:5"]);
}).then($DONE, $DONE);
