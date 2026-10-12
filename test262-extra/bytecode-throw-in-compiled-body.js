/*---
description: >
  throw statements inside hot compiled functions deliver the thrown value
  unchanged to the nearest catch, including primitive values and values
  thrown while other operands are pending on the VM stack.
info: |
  jsse issue #902: throw used to bail the whole function. It now compiles to
  a dedicated opcode, so the operand stack may hold unrelated live values at
  the point of the throw.

  The throw Statement (14.14.1), try Statement (14.15.3).
esid: sec-throw-statement-runtime-semantics-evaluation
---*/

function guard(n) {
  if (n < 0) throw new RangeError("negative");
  return n * 2;
}
assert.sameValue(guard(2), 4);
assert.throws(RangeError, function () { guard(-1); });

function throwsPrimitives(kind) {
  if (kind === 0) throw 42;
  if (kind === 1) throw "str";
  if (kind === 2) throw undefined;
  if (kind === 3) throw null;
  return "none";
}
[0, 1, 2, 3].forEach(function (kind) {
  var caught, threw = false;
  try {
    throwsPrimitives(kind);
  } catch (e) {
    threw = true;
    caught = e;
  }
  assert.sameValue(threw, true, "kind " + kind);
  assert.sameValue(caught, [42, "str", undefined, null][kind], "kind " + kind);
});
assert.sameValue(throwsPrimitives(9), "none");

var marker = { id: "thrown-object" };
function throwsObject() {
  throw marker;
}
try {
  throwsObject();
  throw new Test262Error("unreachable");
} catch (e) {
  assert.sameValue(e, marker);
}

function throwWithPendingOperands() {
  var left = { v: 1 };
  var r = [left, { w: 2 }].length + (function () { throw new SyntaxError("inner"); })();
  return r;
}
assert.throws(SyntaxError, throwWithPendingOperands);

function throwInsideLoopPastFinally() {
  var trace = [];
  try {
    for (var i = 0; i < 5; i++) {
      trace.push(i);
      if (i === 2) throw new EvalError("stop");
    }
  } catch (e) {
    trace.push(e.name);
  }
  return trace.join(",");
}
assert.sameValue(throwInsideLoopPastFinally(), "0,1,2,EvalError");

function hotThrowCatchLoop() {
  var caught = 0;
  for (var i = 0; i < 200; i++) {
    try {
      guard(-1);
    } catch (e) {
      caught++;
    }
  }
  return caught;
}
assert.sameValue(hotThrowCatchLoop(), 200);
