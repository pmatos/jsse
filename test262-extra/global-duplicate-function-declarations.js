/*---
description: >
  Duplicate top-level function declarations of a global script create each name
  once, bound to the last declaration, in the order of the last declarations.
info: |
  GlobalDeclarationInstantiation (sec-globaldeclarationinstantiation) steps 15
  and 16 build functionsToInitialize from the var declarations in reverse order,
  skipping a name that is already present and inserting at the front, then step 17
  calls CreateGlobalFunctionBinding for each.
esid: sec-globaldeclarationinstantiation
flags: [noStrict]
---*/

function first1346() { return "a1"; }
function second1346() { return "b"; }
function first1346() { return "a2"; }

assert.sameValue(first1346(), "a2", "the last declaration wins");

var names = Object.getOwnPropertyNames(globalThis).filter(function (name) {
  return name === "first1346" || name === "second1346";
});
assert.sameValue(names.join(), "second1346,first1346", "created in order of the last declarations");
