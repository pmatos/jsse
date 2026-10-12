/*---
description: Identifier resolution through a large global environment and nested function scopes
info: |
  9.1.2.1 GetIdentifierReference
  6.2.5.5 GetValue
  6.2.5.6 PutValue

  A callee, read or write that is not local is found by walking the outer
  environments. A function redefined between calls must be seen by the next
  call, a missing name is a ReferenceError (typeof is exempt), and strict code
  cannot create a global by assignment.
esid: sec-getidentifierreference
flags: [noStrict]
---*/

// Spill the global environment well past any small-scope fast path.
for (var i = 0; i < 150; i++) {
  this["bulkGlobal" + i] = i;
}
var declaredAfterBulk = "late";

function target() {
  return "first";
}
function callTarget() {
  return target();
}
assert.sameValue(callTarget(), "first");
target = function () {
  return "second";
};
assert.sameValue(callTarget(), "second", "redefined global callee is observed");

function readsBulk() {
  return bulkGlobal0 + bulkGlobal149 + declaredAfterBulk.length;
}
assert.sameValue(readsBulk(), 0 + 149 + 4);

assert.throws(ReferenceError, function () {
  return notDefinedAnywhere;
}, "missing name read");
assert.throws(ReferenceError, function () {
  notDefinedAnywhere();
}, "missing callee");
assert.sameValue(typeof notDefinedAnywhere, "undefined", "typeof of an unresolvable name");

function sloppyWrite() {
  implicitlyCreated = 5;
}
sloppyWrite();
assert.sameValue(implicitlyCreated, 5, "sloppy write creates a global");
assert.sameValue(delete this.implicitlyCreated, true, "created binding is deletable");

function strictWrite() {
  "use strict";
  undeclaredStrictTarget = 1;
}
assert.throws(ReferenceError, strictWrite, "strict write to an undeclared name");

function outer() {
  var counter = 0;
  var step = 2;
  function inner(delta) {
    counter += step + delta;
    counter++;
    return counter;
  }
  return inner;
}
var inc = outer();
assert.sameValue(inc(1), 4);
assert.sameValue(inc(0), 7);

function shadow(bulkGlobal5) {
  return bulkGlobal5;
}
assert.sameValue(shadow("param"), "param", "a parameter shadows a global");
assert.sameValue(bulkGlobal5, 5);
