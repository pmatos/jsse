/*---
description: >
  SpeciesConstructor lookups stay observable when the derived promise is built
  from %Promise%, and a replaced @@species getter, an accessor `constructor`
  or an own `then` still run the general path.
esid: sec-promise.prototype.then
info: |
  Promise.prototype.then step 3 performs SpeciesConstructor(promise, %Promise%),
  and PromiseResolveThenableJob calls `then` on the thenable, which does the
  same. Skipping the native @@species getter is only sound while the getter is
  the original one.
flags: [async]
---*/

var log = [];
var Orig = Promise;

// `constructor` accessor on a native promise returned from a handler
var inner = Promise.resolve("v");
Object.defineProperty(inner, "constructor", {
  get: function () { log.push("ctor-get"); return Orig; }
});
Promise.resolve().then(function () { return inner; })
  .then(function (v) { log.push("outer:" + v); });

// @@species replaced on %Promise%
var calls = 0;
var desc = Object.getOwnPropertyDescriptor(Promise, Symbol.species);
Object.defineProperty(Promise, Symbol.species, {
  get: function () { calls++; return this; },
  configurable: true
});
Promise.resolve(1).then(function () {}).then(function () {});
Promise.resolve(2).finally(function () {});
log.push("species-calls:" + calls);
Object.defineProperty(Promise, Symbol.species, desc);

// a subclass capability is constructed through the subclass
var constructed = 0;
class Counting extends Promise {
  constructor(executor) { constructed++; super(executor); }
}
var counted = Counting.resolve(1);
log.push("after-resolve:" + constructed);
var derived = counted.then(function () {});
log.push("after-then:" + constructed + ":" + (derived instanceof Counting));
Promise.resolve().then(function () { return counted; })
  .then(function () { log.push("after-thenable:" + constructed); });

// own `then` on a native promise is not the intrinsic
var odd = Promise.resolve("odd");
odd.then = function (res) { log.push("own-then"); res("own"); };
Promise.resolve().then(function () { return odd; })
  .then(function (v) { log.push("odd:" + v); });

// a throwing `constructor` accessor rejects the promise being resolved
var bad = Promise.resolve("bad");
Object.defineProperty(bad, "constructor", {
  get: function () { throw "ctor-throws"; }
});
Promise.resolve().then(function () { return bad; })
  .then(function () { log.push("bad-no"); }, function (e) { log.push("bad:" + e); });

var wait = Promise.resolve();
for (var i = 0; i < 16; i++) wait = wait.then(function () {});
wait.then(function () {
  assert.compareArray(log, [
    "species-calls:4", "after-resolve:1", "after-then:2:true",
    "ctor-get", "own-then", "odd:own", "bad:ctor-throws", "outer:v",
    "after-thenable:3"
  ]);
}).then($DONE, $DONE);
