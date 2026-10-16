/*---
description: >
  An async generator resumed from an await continuation runs in the realm of
  the generator function, not in the realm that happens to drain the job queue,
  whether the awaited promise was pending, already settled, or a non-object.
esid: await
info: |
  Await ( value ) step 3-9: the continuation restores the suspended execution
  context, whose Realm is the async generator function's realm.
flags: [async]
features: [cross-realm, async-iteration]
---*/

var other = $262.createRealm().global;
var release;

var source = other.eval(
  "(async function* (mode, gate) {" +
  "  if (mode === 0) { await gate; } else if (mode === 1) { await Promise.resolve(1); } else { await 1; }" +
  "  var literal = [];" +
  "  var error;" +
  "  try { null.x; } catch (e) { error = e; }" +
  "  yield { array: Object.getPrototypeOf(literal) === Array.prototype," +
  "          error: error instanceof TypeError," +
  "          fn: Object.getPrototypeOf(function () {}) === Function.prototype };" +
  "})"
);

var gate = new Promise(function (resolve) { release = resolve; });
var pending = source(0, gate).next();
var settled = source(1, gate).next();
var nonObject = source(2, gate).next();

Promise.resolve().then(function () { release(); });

Promise.all([pending, settled, nonObject]).then(function (results) {
  var labels = ["pending await", "settled await", "non-object await"];
  results.forEach(function (r, i) {
    var v = r.value;
    assert.sameValue(v.array, true, labels[i] + ": array literal uses the generator's realm");
    assert.sameValue(v.error, true, labels[i] + ": thrown TypeError uses the generator's realm");
    assert.sameValue(v.fn, true, labels[i] + ": function literal uses the generator's realm");
  });
}).then($DONE, $DONE);
