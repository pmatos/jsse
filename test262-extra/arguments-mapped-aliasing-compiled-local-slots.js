/*---
description: Mapped arguments aliasing stays coherent for function-local names resolved by slot
info: |
  10.4.4.7 CreateMappedArgumentsObject
  10.4.4.4 [[Set]] / 10.4.4.1 [[Get]] of an arguments exotic object

  The index properties of a sloppy mapped arguments object alias the formal
  parameter bindings in both directions, however the function body reads and
  writes those parameters (plain assignment, compound assignment, update).
esid: sec-createmappedargumentsobject
flags: [noStrict]
---*/

function paramToArguments(a, b) {
  a = 10;
  b += 5;
  a++;
  return arguments[0] * 100 + arguments[1];
}
assert.sameValue(paramToArguments(1, 2), 1107, "parameter writes are visible through arguments");

function argumentsToParam(a, b) {
  arguments[0] = 7;
  arguments[1] = arguments[0] + 1;
  return a * 10 + b;
}
assert.sameValue(argumentsToParam(0, 0), 78, "arguments writes are visible through parameters");

function loopAlias(n) {
  var total = 0;
  for (var i = 0; i < 5; i++) {
    arguments[0] = i;
    total += n;
  }
  return total;
}
assert.sameValue(loopAlias(99), 0 + 1 + 2 + 3 + 4, "alias observed on every iteration");

function unmappedAfterDelete(a) {
  delete arguments[0];
  a = 5;
  return typeof arguments[0];
}
assert.sameValue(unmappedAfterDelete(1), "undefined", "a deleted index is no longer mapped");

function shortCall(a, b) {
  b = 3;
  return arguments.length + ":" + typeof arguments[1];
}
assert.sameValue(shortCall(1), "1:undefined", "an unsupplied parameter is not mapped");

function strictIsUnmapped(a) {
  "use strict";
  a = 9;
  arguments[0] = 4;
  return a + ":" + arguments[0];
}
assert.sameValue(strictIsUnmapped(1), "9:4", "strict functions do not alias");
