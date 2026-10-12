/*---
description: Direct eval var declarations in a function scope with more than sixteen bindings, including deletion
info: |
  19.2.1.3 EvalDeclarationInstantiation
  9.1.1.1.7 DeleteBinding

  Variables created by sloppy direct eval are configurable bindings of the
  function's variable environment and can be deleted; existing bindings are
  unaffected by the growth of the scope.
esid: sec-evaldeclarationinstantiation
flags: [noStrict]
---*/

function many() {
  var a0 = 0, a1 = 1, a2 = 2, a3 = 3, a4 = 4, a5 = 5, a6 = 6, a7 = 7;
  var a8 = 8, a9 = 9, a10 = 10, a11 = 11, a12 = 12, a13 = 13, a14 = 14, a15 = 15;
  var a16 = 16, a17 = 17, a18 = 18, a19 = 19;
  eval("var injected = 'in'; var a3 = 'overwritten';");
  var seen = [injected, a3, a19];
  var removed = delete injected;
  var after = typeof injected;
  eval("var injected2 = 7");
  injected2 += 1;
  return [seen.join(), removed, after, injected2, a0 + a15];
}

var r = many();
assert.sameValue(r[0], "in,overwritten,19");
assert.sameValue(r[1], true, "eval-declared var is deletable");
assert.sameValue(r[2], "undefined", "deleted binding is gone");
assert.sameValue(r[3], 8);
assert.sameValue(r[4], 15);
assert.sameValue(many()[0], "in,overwritten,19", "second call starts from a fresh scope");
