/*---
description: >
  Anonymous function and class values assigned to identifiers receive their
  name from the assignment or declaration even when the surrounding function
  is compiled and the value expression runs on the tree-walker.
info: |
  jsse issue #902: NamedEvaluation is applied by the assignment, not by the
  function expression. A compiled assignment that merely evaluated the
  function expression would produce an empty name.

  IsAnonymousFunctionDefinition (8.4.3), NamedEvaluation (8.4.5).
esid: sec-runtime-semantics-namedevaluation
---*/

function inferredNames() {
  var a = function () {};
  var b = () => {};
  var c = class {};
  var d;
  d = function () {};
  var e;
  e = () => {};
  var f;
  f = class {};
  var g = { prop: function () {} };
  var h = (function () {});
  return [a.name, b.name, c.name, d.name, e.name, f.name, g.prop.name, h.name].join(",");
}
assert.sameValue(inferredNames(), "a,b,c,d,e,f,prop,h");

function namedExpressionKeepsItsOwnName() {
  var a = function own() {};
  var b;
  b = function other() {};
  return a.name + "," + b.name;
}
assert.sameValue(namedExpressionKeepsItsOwnName(), "own,other");

function logicalAssignmentNamesToo() {
  var a;
  a ??= function () {};
  var b = 0;
  b ||= () => {};
  return a.name + "," + b.name;
}
assert.sameValue(logicalAssignmentNamesToo(), "a,b");

function memberAssignmentDoesNotName() {
  var o = {};
  o.m = function () {};
  return o.m.name;
}
assert.sameValue(memberAssignmentDoesNotName(), "");

function hotLoopStillNames() {
  var last;
  for (var i = 0; i < 3; i++) {
    var fn = function () { return i; };
    last = fn.name;
  }
  return last;
}
assert.sameValue(hotLoopStillNames(), "fn");
