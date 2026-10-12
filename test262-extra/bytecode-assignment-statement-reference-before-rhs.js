/*---
description: >
  An assignment in statement context resolves its reference before the
  right-hand side runs and performs PutValue only afterwards, so a global
  property deleted by the right-hand side is re-created in sloppy code and
  throws a ReferenceError in strict code, after the right-hand side's effects.
info: |
  sec-assignment-operators-runtime-semantics-evaluation steps 1.a-1.e:
    lref = Evaluation of LeftHandSideExpression; rval = GetValue(Evaluation of
    AssignmentExpression); PutValue(lref, rval).
  sec-object-environment-records-setmutablebinding-n-v-s: when the binding
  no longer exists and S is true, throw a ReferenceError; else Set(...).
  The compiled bytecode VM folds the trailing Pop into the store op; none of
  that may change the order above.
esid: sec-assignment-operators-runtime-semantics-evaluation
flags: [noStrict]
---*/

var log = [];

function deleteThenReturn(name, value) {
  delete globalThis[name];
  log.push("rhs:" + name);
  return value;
}

function h1() { return deleteThenReturn("g1", 7); }
function h2() { return deleteThenReturn("g2", 8); }
function h3() { log.push("rhs:undeclared3"); return 9; }
function h5() { return deleteThenReturn("g5", 11); }
function h6() { globalThis.newname6 = "created"; log.push("rhs:newname6"); return "rhs"; }
function h7() { return deleteThenReturn("g7", 12); }

globalThis.g1 = 1;
globalThis.g2 = 2;
globalThis.g5 = 5;
globalThis.g7 = 3;

function sloppyStatement() { g1 = h1(); }
function strictStatement() { "use strict"; g2 = h2(); }
function strictUndeclared() { "use strict"; undeclared3 = h3(); }
function forUpdate() { for (var i = 0; i < 1; i++, g5 = h5()) {} }
function sloppyCreate() { newname6 = h6(); }
function varInit() { var x = h7(); g7 = x; }

sloppyStatement();
assert.sameValue(g1, 7, "sloppy: property deleted by the RHS is re-created by PutValue");
assert.sameValue(log.pop(), "rhs:g1");

assert.throws(ReferenceError, strictStatement, "strict: PutValue of a deleted binding throws");
assert.sameValue(log.pop(), "rhs:g2", "the RHS ran before the ReferenceError");
assert.sameValue(Object.prototype.hasOwnProperty.call(globalThis, "g2"), false);

assert.throws(ReferenceError, strictUndeclared, "strict: unresolvable reference throws at PutValue");
assert.sameValue(log.pop(), "rhs:undeclared3", "the RHS ran before the ReferenceError");
assert.sameValue(typeof undeclared3, "undefined");

forUpdate();
assert.sameValue(g5, 11, "for-update assignment re-creates the property");
assert.sameValue(log.pop(), "rhs:g5");

sloppyCreate();
assert.sameValue(newname6, "rhs", "unresolvable at ResolveBinding, created by the RHS, set by PutValue");
assert.sameValue(log.pop(), "rhs:newname6");

varInit();
assert.sameValue(g7, 12);
assert.sameValue(log.pop(), "rhs:g7");
assert.sameValue(log.length, 0);
