/*---
esid: sec-delete-operator-runtime-semantics-evaluation
description: >
  `delete o?.m(args)` evaluates the call and yields true. The callee, receiver
  and argument values stay reachable while argument evaluation runs user code
  that collects, and the temporary roots taken for the arguments are released
  before the surrounding expression continues.
features: [optional-chaining, host-gc-required]
---*/

var seen = [];
var o = {
  m: function () {
    seen.push(arguments.length, this === o, arguments[0].v, arguments[1].v);
  },
};

function arg(v) {
  $262.gc();
  return { v: v };
}

assert.sameValue(delete o?.m(arg(1), arg(2)), true, "the delete expression yields true");
assert.sameValue(seen.length, 4, "the call ran once");
assert.sameValue(seen[0], 2, "both arguments were passed");
assert.sameValue(seen[1], true, "the receiver is the chain base");
assert.sameValue(seen[2], 1, "first argument survived the collection");
assert.sameValue(seen[3], 2, "second argument survived the collection");

({});
var combined = "x" + delete o?.m(arg(3), arg(4));
assert.sameValue(combined, "xtrue", "an id-rooted operand to the left of the delete is unaffected");
assert.sameValue(seen[6], 3, "arguments of the second call survived");
