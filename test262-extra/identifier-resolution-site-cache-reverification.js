/*---
description: A free identifier read keeps resolving to the innermost binding after the scope chain it was warmed on changes
info: |
  9.4.2 ResolveBinding
  9.1.2.1 GetIdentifierReference
  9.1.1.2.1 Object Environment Record HasBinding
  19.2.1.3 PerformEval

  Resolution walks the environment chain from the running environment outwards
  on every evaluation. A read that has resolved to the same scope many times
  must still find a binding introduced in a nearer scope afterwards (by a
  direct eval `var`, a `with` object property or a TDZ binding becoming
  initialised), and must keep working when bindings are deleted and the
  remaining ones change position.
esid: sec-resolvebinding
flags: [noStrict]
---*/

function directEvalShadowsAfterWarmup() {
  var x = "outer";
  function mid() {
    function inner() { return x; }
    var seen = [];
    for (var i = 0; i < 4; i++) seen.push(inner());
    eval("var x = 'eval'");
    seen.push(inner());
    return seen.join();
  }
  return mid();
}
assert.sameValue(
  directEvalShadowsAfterWarmup(),
  "outer,outer,outer,outer,eval",
  "a var introduced by direct eval in an intermediate function shadows the warmed binding"
);

var withTarget = "global";
var withObject = {};
var readThroughWith;
with (withObject) {
  readThroughWith = function () { return withTarget; };
}
for (var warm = 0; warm < 4; warm++) readThroughWith();
withObject.withTarget = "with";
assert.sameValue(readThroughWith(), "with", "a property added to a with object is found");
delete withObject.withTarget;
assert.sameValue(readThroughWith(), "global", "and lost again once deleted");

function tdzThenInitialised() {
  function read() { return v; }
  var firstError = null;
  try {
    read();
  } catch (e) {
    firstError = e;
  }
  let v = 1;
  return [firstError instanceof ReferenceError, read(), read()].join();
}
assert.sameValue(tdzThenInitialised(), "true,1,1", "captured let throws in its TDZ, then reads normally");

function deletedBindingsShiftSlots() {
  var a = 1;
  eval("var b = 2; var c = 3;");
  function readC() { return c; }
  function readB() { return typeof b; }
  var seen = [readC(), readB(), readC()];
  delete b;
  seen.push(readC(), readB());
  delete c;
  seen.push(typeof c);
  return seen.join();
}
assert.sameValue(
  deletedBindingsShiftSlots(),
  "3,number,3,3,undefined,undefined",
  "reads stay correct while eval-declared bindings are deleted"
);

function sameLiteralDifferentDepths() {
  var results = [];
  function make(extra) {
    var value = "level-" + extra;
    if (extra) {
      return (function () { return (function () { return value; })(); })();
    }
    return (function () { return value; })();
  }
  results.push(make(0), make(1), make(0), make(1));
  return results.join();
}
assert.sameValue(
  sameLiteralDifferentDepths(),
  "level-0,level-1,level-0,level-1",
  "closures at different nesting depths resolve the same name correctly"
);

function nearerBindingAppearsInLoop() {
  var v = "far";
  var reads = [];
  function read() { return v; }
  for (var i = 0; i < 3; i++) {
    reads.push(read());
  }
  {
    let v = "near";
    reads.push(read());
    reads.push(v);
  }
  return reads.join();
}
assert.sameValue(
  nearerBindingAppearsInLoop(),
  "far,far,far,far,near",
  "a block-scoped binding in the caller does not capture the callee's free name"
);
