/*---
description: >
  An async function resumed from an await continuation runs in the realm of
  the function, not in the realm that happens to drain the job queue, whether
  the awaited promise was pending or already settled.
esid: await
info: |
  Await ( value ) step 3-9: the continuation restores the suspended execution
  context, whose Realm is the async function's realm.
flags: [async]
features: [cross-realm]
---*/

var other = $262.createRealm().global;
var release;

var source = other.eval(
  "(async function (settled, gate) {" +
  "  var viaPending;" +
  "  if (settled) { await 1; } else { await gate; }" +
  "  var literal = [];" +
  "  var error;" +
  "  try { null.x; } catch (e) { error = e; }" +
  "  return { array: Object.getPrototypeOf(literal) === Array.prototype," +
  "           error: error instanceof TypeError," +
  "           fn: Object.getPrototypeOf(function () {}) === Function.prototype };" +
  "})"
);

var gate = new Promise(function (resolve) { release = resolve; });
var pendingResult = source(false, gate);
var settledResult = source(true, gate);

Promise.resolve().then(function () { release(); });

Promise.all([pendingResult, settledResult]).then(function (results) {
  results.forEach(function (r, i) {
    var label = i === 0 ? "pending await" : "settled await";
    assert.sameValue(r.array, true, label + ": array literal uses the function's realm");
    assert.sameValue(r.error, true, label + ": thrown TypeError uses the function's realm");
    assert.sameValue(r.fn, true, label + ": function literal uses the function's realm");
  });
}).then($DONE, $DONE);
