/*---
description: Calls in nested tail positions are proper tail calls and keep short-circuit, ordering and this semantics
esid: sec-static-semantics-hascallintailposition
info: |
  HasCallInTailPosition reaches a call through the branches of a
  ConditionalExpression, the right operand of LogicalANDExpression,
  LogicalORExpression and CoalesceExpression, and the last operand of a
  CommaExpression. Each such call is a tail call in strict code and must run
  in constant stack. Operands that are not in tail position are evaluated
  first and in order; the left operand of a short-circuit operator is the
  result when it short-circuits; a member callee keeps its base as this; a
  throw from the leaf reaches an enclosing try; a constructor whose return
  expression yields a non-object still produces this.
flags: [onlyStrict]
features: [tail-call-optimization]
---*/

"use strict";

var N = 100000;

// Deep recursion through each nested shape, with and without a native prefix.
function viaAnd(n, acc) { return n > 0 && viaAnd(n - 1, acc + 1) || acc; }
function viaAndOnly(n) { return n === 0 || viaAndOnly(n - 1); }
function viaOr(n) { return n === 0 ? true : false || viaOr(n - 1); }
function viaNullish(n) { return n === 0 ? 7 : null ?? viaNullish(n - 1); }
function viaCond(n) { return n === 0 ? "done" : viaCond(n - 1); }
function viaCondElse(n) { return n !== 0 ? viaCondElse(n - 1) : "done"; }
function viaComma(n) { return n === 0 ? "done" : (n, viaComma(n - 1)); }
function viaNested(n) {
  var t = n & 3;
  return n === 0 ? "done" : t > 1 ? (t, true && viaNested(n - 1)) : false || viaNested(n - 1);
}

assert.sameValue(viaAndOnly(N), true, "&& right operand recurses in constant stack");
assert.sameValue(viaOr(N), true, "|| right operand recurses in constant stack");
assert.sameValue(viaNullish(N), 7, "?? right operand recurses in constant stack");
assert.sameValue(viaCond(N), "done", "consequent recurses in constant stack");
assert.sameValue(viaCondElse(N), "done", "alternate recurses in constant stack");
assert.sameValue(viaComma(N), "done", "last comma operand recurses in constant stack");
assert.sameValue(viaNested(N), "done", "mixed nesting recurses in constant stack");
assert.sameValue(viaAnd(10, 0), 10, "a non-tail use of the same shape still computes");

// Mutual recursion across two functions.
function even(n) { return n === 0 ? true : odd(n - 1); }
function odd(n) { return n === 0 ? false : even(n - 1); }
assert.sameValue(even(N), true, "mutual recursion through ?: is a tail call");

// Spread arguments in the leaf keep tail-call behaviour.
function spreadLeaf(n, args) { return n === 0 ? "done" : spreadLeaf(...[n - 1, args]); }
assert.sameValue(spreadLeaf(N, []), "done", "a spread leaf is a tail call");
function spreadAnd(n, args) { return n === 0 || spreadAnd(...[n - 1, args]); }
assert.sameValue(spreadAnd(N, []), true, "a spread leaf after && is a tail call");

// Short-circuit completion values: the left operand is the result.
var calls = 0;
function leaf() { calls++; return "leaf"; }
function f1(a) { return a && leaf(); }
function f2(a) { return a || leaf(); }
function f3(a) { return a ?? leaf(); }
assert.sameValue(f1(0), 0, "&& returns a falsy left operand");
assert.sameValue(f1(""), "", "&& returns an empty string left operand");
assert.sameValue(f1(1), "leaf", "&& returns the call result");
assert.sameValue(f2("x"), "x", "|| returns a truthy left operand");
assert.sameValue(f2(0), "leaf", "|| returns the call result");
assert.sameValue(f3(0), 0, "?? keeps a falsy non-nullish left operand");
assert.sameValue(f3(undefined), "leaf", "?? calls for undefined");
assert.sameValue(f3(null), "leaf", "?? calls for null");
assert.sameValue(calls, 4, "the leaf runs only when the operator does not short-circuit");

// Evaluation order of the non-tail operands and of the callee.
var log = [];
function note(x) { log.push(x); return x; }
var fa = function (x) { log.push("fa" + x); return "ra"; };
var fc = function (x) { log.push("fc" + x); return "rc"; };
function order2(c) { return (note("t"), c) ? (note("p"), fa)(note(1)) : fc(note(2)); }
assert.sameValue(order2(true), "ra", "consequent result");
assert.sameValue(log.join(), "t,p,1,fa1", "test, callee and argument order");
log = [];
assert.sameValue(order2(false), "rc", "alternate result");
assert.sameValue(log.join(), "t,2,fc2", "alternate order");
log = [];
function order3(x) { return note(x), note(x + 1), fa(note(x + 2)); }
assert.sameValue(order3(1), "ra", "comma result");
assert.sameValue(log.join(), "1,2,3,fa3", "comma prefix runs before the leaf");

// this of member callees and indirect callees.
var obj = {
  tag: "obj",
  who: function () { return this === undefined ? "undefined" : this.tag; },
  viaAnd: function (c) { return c && this.who(); },
  viaCond: function (c) { return c ? this.who() : null; },
  viaComma: function () { return 0, this.who(); },
  viaIndirect: function () { return 0, (0, this.who)(); },
  viaComputed: function (k) { return k ? this[k]() : null; }
};
assert.sameValue(obj.viaAnd(true), "obj", "member callee after && keeps its base");
assert.sameValue(obj.viaCond(true), "obj", "member callee in ?: keeps its base");
assert.sameValue(obj.viaComma(), "obj", "member callee after comma keeps its base");
assert.sameValue(obj.viaIndirect(), "undefined", "indirect callee has undefined this");
assert.sameValue(obj.viaComputed("who"), "obj", "computed member callee keeps its base");

// An exception from the leaf reaches an enclosing try in the caller and in the same function.
function thrower() { throw new Error("boom"); }
function catcher(c) {
  try {
    return c && thrower();
  } catch (e) {
    return "caught:" + e.message;
  }
}
assert.sameValue(catcher(true), "caught:boom", "a throw from an && leaf reaches the try");
assert.sameValue(catcher(0), 0, "a short-circuited && inside try returns the left operand");
function finalizer(log, c) {
  try {
    return c ? thrower() : "none";
  } catch (e) {
    return "caught";
  } finally {
    log.push("finally");
  }
}
var flog = [];
assert.sameValue(finalizer(flog, true), "caught", "a throw from a ?: leaf reaches catch");
assert.sameValue(finalizer(flog, false), "none", "a non-throwing branch returns normally");
assert.sameValue(flog.join(), "finally,finally", "finally runs once per call");
function outer(c) {
  try {
    return inner(c);
  } catch (e) {
    return "outer:" + e.message;
  }
}
function inner(c) { return c ? thrower() : "ok"; }
assert.sameValue(outer(true), "outer:boom", "a throw from a tail leaf propagates to the caller's try");
assert.sameValue(outer(false), "ok", "a tail leaf returns its value to the caller");

// A call returned out of a for-in body is an ordinary call.
function inForIn(o, c) {
  for (var k in o) {
    return c && leaf();
  }
}
assert.sameValue(inForIn({ a: 1 }, true), "leaf", "a call returned out of for-in produces its value");

// new and construct tail positions are not call tail positions.
function Ctor(c) { return c ? leaf() : 0; }
var made = new Ctor(true);
assert.sameValue(typeof made, "object", "a constructor returning a non-object yields this");
assert.sameValue(Object.getPrototypeOf(made), Ctor.prototype, "the result is the new object");
function Ctor2(c) { return c && Object; }
assert.sameValue(new Ctor2(true), Object, "a constructor returning an object yields that object");
assert.sameValue(Object.getPrototypeOf(new Ctor2(false)), Ctor2.prototype, "a falsy value yields this");
function Ctor3(c) { return c ? new Object() : leaf(); }
assert.sameValue(typeof new Ctor3(false), "object", "a call leaf in a constructor yields this for a non-object");
class Base { constructor() { return undefined; } }
class Derived extends Base {
  constructor(c) { super(); return c ? undefined : leaf(); }
}
assert.sameValue(typeof new Derived(true), "object", "derived constructor returning undefined yields this");
assert.throws(TypeError, function () { new Derived(false); }, "derived constructor returning a primitive throws");

// Operands that are not in tail position stay ordinary calls.
function nonTail(c) { return (c ? leaf() : leaf()) + "!"; }
assert.sameValue(nonTail(true), "leaf!", "a call inside a binary operand is evaluated normally");
function nonTailArg(c) { return String(c && leaf()); }
assert.sameValue(nonTailArg(true), "leaf", "a call inside an argument is evaluated normally");

// Tagged templates keep their tail-call behaviour.
function tag(strings) { return strings[0]; }
function taggedLeaf(n) { return n === 0 ? tag`done` : taggedLeaf(n - 1); }
assert.sameValue(taggedLeaf(N), "done", "recursion through a conditional with a tagged template leaf");
function tagLeaf(c) { return c && tag`x`; }
assert.sameValue(tagLeaf(true), "x", "a tagged template leaf after &&");
assert.sameValue(tagLeaf(false), false, "a short-circuited tagged template leaf");
