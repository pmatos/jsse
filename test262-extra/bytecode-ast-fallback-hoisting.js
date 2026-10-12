/*---
description: >
  var, function and Annex B function-in-block hoisting stay observable when
  the statements that declare them run on the tree-walker inside a compiled
  function.
info: |
  jsse issue #902: the VM prologue only declares the vars it compiles itself.
  Function declarations, top-level lexical bindings and vars declared inside
  statements handed back to the tree-walker must still be instantiated
  before the body runs.

  FunctionDeclarationInstantiation (10.2.11), Block-Level Function
  Declarations Web Legacy Compatibility Semantics (B.3.2.1).
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function varDeclaredInsideTryIsUndefinedBefore() {
  var before = typeof inner;
  try {
    var inner = 1;
  } catch (e) {}
  return before + ":" + inner;
}
assert.sameValue(varDeclaredInsideTryIsUndefinedBefore(), "undefined:1");

function varInForInIsHoisted() {
  var before = key;
  for (var key in { a: 1 }) {}
  return before + ":" + key;
}
assert.sameValue(varInForInIsHoisted(), "undefined:a");

function topLevelFunctionCallableBeforeDefinition() {
  var r = helper(2);
  function helper(n) { return n * 21; }
  return r;
}
assert.sameValue(topLevelFunctionCallableBeforeDefinition(), 42);

function annexBFunctionInBlockVisibleAfterBlock() {
  var before = typeof inBlock;
  {
    function inBlock() { return "block"; }
  }
  return before + ":" + inBlock();
}
assert.sameValue(annexBFunctionInBlockVisibleAfterBlock(), "undefined:block");

function annexBFunctionInIfBranch() {
  if (true) {
    function chosen() { return "yes"; }
  }
  return chosen();
}
assert.sameValue(annexBFunctionInIfBranch(), "yes");

function parameterAndVarShareBinding(p) {
  var p;
  for (var i = 0; i < 2; i++) {
    try { p = p + 1; } catch (e) {}
  }
  return p;
}
assert.sameValue(parameterAndVarShareBinding(1), 3);

function argumentsObjectInsideFallback() {
  var total = 0;
  for (var v of arguments) total += v;
  return total + ":" + arguments.length;
}
assert.sameValue(argumentsObjectInsideFallback(1, 2, 3), "6:3");

function thisInsideFallback() {
  var seen = [];
  [1].forEach(function () { seen.push(typeof this); });
  var self = this;
  try { seen.push(self === this); } catch (e) {}
  return seen.join(",");
}
assert.sameValue(thisInsideFallback.call({}), "object,true");
