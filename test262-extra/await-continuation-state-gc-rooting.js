/*---
description: >
  A suspended async function stays fully reachable while its `await`
  continuation is a typed reaction: locals, the active for-of iterator, the
  per-iteration scope, a pending `finally`, and the thenable or promise being
  awaited survive collections that run while the function is parked.
esid: await
info: |
  Await ( value ) saves the running execution context and resumes it from a
  PromiseReaction job. The engine keeps the saved state in its scheduler, and
  the reaction carries no handler function that could root it.
flags: [async]
features: [host-gc-required]
---*/

var release;
var releaseSecond;
var order = [];

function* source() {
  try {
    yield 1;
    yield 2;
    yield 3;
  } finally {
    order.push("source-closed");
  }
}

async function worker() {
  var box = { n: 40 };
  var closures = [];
  try {
    for (const v of source()) {
      let local = { v: v, tag: "t" + v };
      closures.push(function () { return local.tag + ":" + box.n; });
      if (v === 1) {
        box.n += await new Promise(function (r) { release = r; });
      } else if (v === 2) {
        box.n += await {
          then: function (resolve) {
            $262.gc();
            resolve(1);
          }
        };
        $262.gc();
      } else {
        box.n += await new Promise(function (r) { releaseSecond = r; });
      }
      $262.gc();
    }
  } finally {
    order.push("finally:" + box.n);
    await null;
    $262.gc();
    order.push("finally-after-await");
  }
  return closures.map(function (f) { return f(); }).join(",");
}

var result = worker();

$262.gc();
release(1);
Promise.resolve().then(function () {}).then(function () {}).then(function () {}).then(function () {
  $262.gc();
  releaseSecond(1);
});

result.then(function (v) {
  assert.sameValue(v, "t1:43,t2:43,t3:43");
  assert.compareArray(order, ["source-closed", "finally:43", "finally-after-await"]);
}).then($DONE, $DONE);
