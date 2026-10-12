/*---
description: >
  A closure that reads a global var keeps working when an enclosing function
  later creates a same-named var via sloppy direct eval: the closure sees the
  eval-declared binding while other callers still see the global.
info: |
  EvalDeclarationInstantiation (sec-evaldeclarationinstantiation) adds the
  var to the function's variable environment, which precedes the global
  environment in the scope chain.
esid: sec-evaldeclarationinstantiation
flags: [noStrict]
---*/

var ev = "global";
function makeReader() {
  return function () { return ev; };
}
var plain = makeReader();
for (var i = 0; i < 10; i++) { assert.sameValue(plain(), "global"); }

function outer() {
  var inner = function () { return ev; };
  for (var i = 0; i < 10; i++) { assert.sameValue(inner(), "global"); }
  eval("var ev = 'eval';");
  assert.sameValue(inner(), "eval", "closure sees eval-declared var");
  assert.sameValue(ev, "eval");
  ev = "assigned";
  assert.sameValue(inner(), "assigned");
}
outer();
assert.sameValue(ev, "global", "global untouched");
assert.sameValue(plain(), "global");
