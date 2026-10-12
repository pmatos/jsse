/*---
description: >
  Promises derived by `then` on %Promise% stay reachable from the reactions of
  the promise they derive from, including a derived promise that is locked in
  to a still-pending native promise, across collections.
esid: sec-performpromisethen
info: |
  The derived promise's resolving functions are never created for %Promise%,
  so nothing but the PromiseReaction record references it. It must survive
  collections while the source promise is pending and while it waits on a
  native promise returned from a handler.
flags: [async]
features: [host-gc-required]
---*/

var release;
var late;
var out = [];
var finished = 0;

function finish() {
  if (++finished === 2) {
    assert.compareArray(out.slice().sort(), ["late:L!", "then:8"]);
    $DONE();
  }
}

(function () {
  var gate = new Promise(function (r) { release = r; });
  gate.then(function (v) { $262.gc(); return { n: v + 1 }; })
    .then(function (o) { $262.gc(); out.push("then:" + o.n); })
    .then(finish, $DONE);
  gate.then(function () { return new Promise(function (r) { late = r; }); })
    .then(function (v) { out.push("late:" + v + "!"); })
    .then(finish, $DONE);
})();

$262.gc();
release(7);
Promise.resolve().then(function () {}).then(function () {}).then(function () {
  $262.gc();
  late("L");
});
