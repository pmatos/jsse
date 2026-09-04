/*---
description: A call in the try Block of a try statement is not in tail position
esid: sec-static-semantics-hascallintailposition
info: |
  For `TryStatement : try Block Catch`, HasCallInTailPosition returns
  HasCallInTailPosition of Catch. It does not inspect the try Block, because
  the exception handler must remain active while that Block executes.

  A call returned from the try Block must therefore remain an ordinary call,
  even in strict mode. A throw from that call must reach the statement's catch
  handler. Proper tail calls outside the try Block must remain optimized.
flags: [onlyStrict]
features: [tail-call-optimization]
---*/

"use strict";

function thrower() {
  throw new Error("boom");
}

function wrap(f) {
  try {
    return f();
  } catch (e) {
    return "caught:" + e.message;
  }
}

var r = wrap(thrower);
assert.sameValue(r, "caught:boom", "the catch handler remains active for a call in the try Block");

function count(n, acc) {
  "use strict";
  if (n <= 0) return acc;
  return count(n - 1, acc + 1);
}

assert.sameValue(count(200000, 0), 200000, "a proper tail call outside a try Block remains optimized");
