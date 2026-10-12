/*---
description: >
  Reaction jobs and thenable jobs run in the order the specification fixes
  when the derived promise is the unexposed capability of %Promise%.
esid: sec-performpromisethen
info: |
  The result capability built from %Promise% has resolving functions nothing
  can observe, so an implementation may settle the derived promise without
  creating them. The number of jobs between the settlement of a promise and
  the settlement of the promise derived from it must not change:
  returning a native promise from a handler costs a PromiseResolveThenableJob
  plus one reaction job on the returned promise, and a `then` on a settled
  promise queues its job at once.
flags: [async]
---*/

var log = [];
function mark(s) { log.push(s); }

// native promise returned from a handler: thenable job + one reaction job
Promise.resolve().then(function () { mark("a1"); return Promise.resolve("x"); })
  .then(function (v) { mark("a2:" + v); });
// plain value chain as a tick counter
Promise.resolve().then(function () { mark("b1"); }).then(function () { mark("b2"); })
  .then(function () { mark("b3"); }).then(function () { mark("b4"); })
  .then(function () { mark("b5"); });
// resolve function called with a native promise
new Promise(function (r) { r(Promise.resolve("y")); }).then(function (v) { mark("c:" + v); });
// async function returning a native promise, and a plain value
(async function () { return Promise.resolve("z"); })().then(function (v) { mark("d:" + v); });
(async function () { return "w"; })().then(function (v) { mark("e:" + v); });
// PromiseResolve of a promise whose constructor is %Promise% is the identity
var base = Promise.resolve(1);
mark("f:" + (Promise.resolve(base) === base));
// rejection forwarded through an empty fulfillment handler
Promise.reject("r").then(function () { mark("g-no"); }).catch(function (e) { mark("g:" + e); });
// thenable whose then throws
Promise.resolve().then(function () { return { then: function () { throw "t"; } }; })
  .catch(function (e) { mark("h:" + e); });
// native promise returned from a rejection handler
Promise.reject("q").catch(function (e) { return Promise.reject(e + "!"); })
  .catch(function (e) { mark("i:" + e); });
// await of a native promise and of a thenable
(async function () {
  mark("j0");
  await Promise.resolve();
  mark("j1");
  await { then: function (r) { r(); } };
  mark("j2");
})();
Promise.resolve("v").finally(function () { mark("k0"); }).then(function (v) { mark("k:" + v); });

var wait = Promise.resolve();
for (var i = 0; i < 16; i++) wait = wait.then(function () {});
wait.then(function () {
  assert.compareArray(log, [
    "f:true", "j0", "a1", "b1", "e:w", "j1", "k0", "b2", "g:r", "b3", "c:y",
    "d:z", "h:t", "j2", "a2:x", "b4", "i:q!", "k:v", "b5"
  ]);
}).then($DONE, $DONE);
