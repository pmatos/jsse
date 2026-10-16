// Copyright (C) 2026 the JSSE authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
description: >
  === / !== (as an expression, as an if condition, and as a switch case test)
  on non-Number operands: Symbols compared by identity including ones rebuilt
  from their own-key enumeration, strings by content across distinct
  allocations and substring/concat views, BigInt by value, mixed kinds,
  null/undefined/booleans, objects by identity, and $262.IsHTMLDDA staying
  falsy as a condition while comparing by identity.
info: |
  jsse issue #1059: the bytecode VM compares non-Number pairs inline and
  switch case tests use a fused compare-and-jump op. Both must behave exactly
  like IsStrictlyEqual (7.2.15), ToBoolean (7.1.2), and CaseClauseIsSelected
  (14.12.3), in a function body (bytecode) and in a generator body
  (tree-walker under default flags).
esid: sec-isstrictlyequal
features: [Symbol, BigInt, Reflect, generators, IsHTMLDDA]
includes: [compareArray.js]
---*/

function eqExpr(a, b) { return [a === b, a !== b]; }
function eqIf(a, b) {
  var r = "";
  if (a === b) r += "E"; else r += "e";
  if (a !== b) r += "N"; else r += "n";
  return r;
}
function eqSwitch(v, k) {
  switch (v) {
    case k: return "match";
    default: return "default";
  }
}
function* gEqExpr(a, b) { yield [a === b, a !== b]; }
function* gEqIf(a, b) {
  var r = "";
  if (a === b) r += "E"; else r += "e";
  if (a !== b) r += "N"; else r += "n";
  yield r;
}
function* gEqSwitch(v, k) {
  switch (v) {
    case k: yield "match"; break;
    default: yield "default";
  }
}

// Each check runs through all six shapes.
function check(a, b, same, message) {
  var expr = same ? [true, false] : [false, true];
  assert.compareArray(eqExpr(a, b), expr, message + " (expr)");
  assert.compareArray(gEqExpr(a, b).next().value, expr, message + " (generator expr)");
  assert.sameValue(eqIf(a, b), same ? "En" : "eN", message + " (if)");
  assert.sameValue(gEqIf(a, b).next().value, same ? "En" : "eN", message + " (generator if)");
  assert.sameValue(eqSwitch(a, b), same ? "match" : "default", message + " (switch)");
  assert.sameValue(gEqSwitch(a, b).next().value, same ? "match" : "default", message + " (generator switch)");
}

// Symbols: identity, and symbols rebuilt by own-key enumeration.
var s1 = Symbol("s");
var s2 = Symbol("s");
var holder = {};
holder[s1] = 1;
holder[s2] = 2;
var owned = Object.getOwnPropertySymbols(holder);
var reflected = Reflect.ownKeys(holder).filter(function (k) { return typeof k === "symbol"; });
check(s1, s1, true, "symbol with itself");
check(s1, s2, false, "distinct symbols with one description");
check(owned[0], s1, true, "getOwnPropertySymbols[0] vs original");
check(s1, owned[0], true, "original vs getOwnPropertySymbols[0]");
check(owned[1], s2, true, "getOwnPropertySymbols[1] vs original");
check(owned[0], s2, false, "getOwnPropertySymbols[0] vs the other original");
check(reflected[0], s1, true, "Reflect.ownKeys symbol vs original");
check(reflected[1], owned[1], true, "Reflect.ownKeys symbol vs getOwnPropertySymbols symbol");
check(Symbol.for("reg"), Symbol.for("reg"), true, "Symbol.for twice");
check(Symbol.for("reg"), Symbol("reg"), false, "Symbol.for vs Symbol");
check(Symbol.iterator, Symbol.iterator, true, "well-known symbol");
check(Symbol.iterator, Symbol.asyncIterator, false, "two well-known symbols");
var registered = {};
registered[Symbol.for("k")] = 1;
registered[Symbol.iterator] = 2;
var enumerated = Object.getOwnPropertySymbols(registered);
check(enumerated[0], Symbol.for("k"), true, "enumerated registered symbol");
check(enumerated[1], Symbol.iterator, true, "enumerated well-known symbol");
check(s1, "s", false, "symbol vs its description");
check(s1, Object(s1), false, "symbol vs its wrapper");

// Strings: content, not allocation.
var base = "prefix-";
var built = base + "suffix";
var literal = "prefix-suffix";
var joined = ["prefix", "suffix"].join("-");
check(built, literal, true, "concat vs literal");
check(joined, literal, true, "join vs literal");
check(literal.slice(0, 6), "prefix", true, "slice vs literal");
check(literal.substring(7), "suffix", true, "substring vs literal");
check(literal.slice(0, 6), literal.slice(0, 7), false, "different lengths, shared buffer");
check(literal.slice(1, 7), "refix-", true, "interior view vs literal");
check(literal.slice(1, 7), "prefix", false, "interior view vs shifted literal");
var grown = "a";
for (var i = 0; i < 40; i++) grown += "b";
var grownPrefix = grown + "c";
var grownOther = grown + "d";
check(grownPrefix, grownOther, false, "two appends to one buffer");
check(grownPrefix, grown + "c", true, "append re-created");
check(grownPrefix.slice(0, 41), grown, true, "append view prefix vs original");
check("", "", true, "empty strings");
check("", String(""), true, "empty string vs String()");
check("", "\0", false, "empty vs NUL");
check("\uD83D", "\uD83D", true, "lone surrogate");
check("😀", "\u{1F600}", true, "surrogate pair vs code point escape");
check("\uD83D", "\uDE00", false, "different lone surrogates");
check("1", 1, false, "string vs number");
check(1, "1", false, "number vs string");
check("a", new String("a"), false, "string vs wrapper");
check("true", true, false, "string vs boolean");

// Numbers: the IEEE rules stay with the Number pair.
check(NaN, NaN, false, "NaN vs NaN");
check(0, -0, true, "+0 vs -0");
check(-0, 0, true, "-0 vs +0");
check(1, 1, true, "equal integers");
check(0.1 + 0.2, 0.3, false, "inexact sum");
check(1, 1n, false, "number vs BigInt");
check(1n, 1, false, "BigInt vs number");
check(NaN, "NaN", false, "NaN vs string");

// BigInt: by value.
check(1n, 1n, true, "equal BigInts");
check(2n ** 64n, 2n ** 64n, true, "equal large BigInts, distinct allocations");
check(2n ** 64n, 2n ** 64n + 1n, false, "adjacent large BigInts");
check(-1n, 1n, false, "sign differs");
check(0n, -0n, true, "BigInt zero");
check(BigInt(Number.MAX_SAFE_INTEGER) + 2n, 9007199254740993n, true, "computed vs literal");
check(1n, "1", false, "BigInt vs string");
check(1n, true, false, "BigInt vs boolean");
check(1n, Object(1n), false, "BigInt vs wrapper");

// null, undefined, booleans.
check(null, null, true, "null vs null");
check(undefined, undefined, true, "undefined vs undefined");
check(void 0, undefined, true, "void 0 vs undefined");
check(null, undefined, false, "null vs undefined");
check(undefined, null, false, "undefined vs null");
check(true, true, true, "true vs true");
check(false, false, true, "false vs false");
check(true, false, false, "true vs false");
check(false, 0, false, "false vs 0");
check(false, "", false, "false vs empty string");
check(false, null, false, "false vs null");
check(true, 1, false, "true vs 1");
check(null, 0, false, "null vs 0");
check(undefined, 0, false, "undefined vs 0");
check(undefined, "undefined", false, "undefined vs string");
check(null, {}, false, "null vs object");

// Objects: identity only, valueOf and toString never run.
var calls = 0;
var o1 = { valueOf: function () { calls++; return 1; }, toString: function () { calls++; return "x"; } };
var o2 = { valueOf: o1.valueOf, toString: o1.toString };
check(o1, o1, true, "object with itself");
check(o1, o2, false, "distinct objects with equal shape");
check(o1, 1, false, "object vs its valueOf");
check(o1, "x", false, "object vs its toString");
check([], [], false, "two arrays");
var fn = function () {};
check(fn, fn, true, "function with itself");
check(fn.bind(), fn, false, "bound function vs target");
check(Object(1), Object(1), false, "two wrappers");
assert.sameValue(calls, 0, "=== must not call valueOf/toString");

// $262.IsHTMLDDA compares by identity but is falsy as a condition.
var dda = $262.IsHTMLDDA;
check(dda, dda, true, "IsHTMLDDA with itself");
check(dda, undefined, false, "IsHTMLDDA vs undefined");
check(dda, null, false, "IsHTMLDDA vs null");
check(undefined, dda, false, "undefined vs IsHTMLDDA");
check(dda, "", false, "IsHTMLDDA vs empty string");
check(dda, {}, false, "IsHTMLDDA vs object");

function truthiness(v) {
  var r = "";
  if (v) r += "T"; else r += "F";
  if (!v) r += "N"; else r += "Y";
  r += v ? "T" : "F";
  r += (v && 1) ? "T" : "F";
  while (v) { r += "W"; break; }
  return r;
}
assert.sameValue(truthiness(dda), "FNFF", "IsHTMLDDA is falsy as a condition");
assert.sameValue(truthiness(true), "TYTTW", "true");
assert.sameValue(truthiness(false), "FNFF", "false");
assert.sameValue(truthiness(null), "FNFF", "null");
assert.sameValue(truthiness(undefined), "FNFF", "undefined");
assert.sameValue(truthiness(""), "FNFF", "empty string");
assert.sameValue(truthiness("x"), "TYTTW", "non-empty string");
assert.sameValue(truthiness(0n), "FNFF", "0n");
assert.sameValue(truthiness(1n), "TYTTW", "1n");
assert.sameValue(truthiness(NaN), "FNFF", "NaN");
assert.sameValue(truthiness({}), "TYTTW", "plain object");
assert.sameValue(truthiness(Symbol()), "TYTTW", "symbol");

function switchOnCondition(v) {
  switch (true) {
    case v: return "case-true-selector";
    default: return "default";
  }
}
assert.sameValue(switchOnCondition(true), "case-true-selector", "switch(true) with true");
assert.sameValue(switchOnCondition(1), "default", "switch(true) with 1");
assert.sameValue(switchOnCondition(dda), "default", "switch(true) with IsHTMLDDA");

// switch: lazy in-order selectors, fall-through, default placement.
function order(v, trace) {
  var out = [];
  switch (v) {
    case (trace.push("a"), Symbol.iterator): out.push("a");
    case (trace.push("b"), "str"): out.push("b"); break;
    default: out.push("d");
    case (trace.push("c"), 7n): out.push("c"); break;
    case (trace.push("e"), o1): out.push("e");
  }
  return out.join("");
}
var trace = [];
assert.sameValue(order(Symbol.iterator, trace), "ab", "first symbol selector");
assert.sameValue(trace.join(""), "a", "selectors after the match are not evaluated");
trace = [];
assert.sameValue(order("st" + "r", trace), "b", "second selector");
assert.sameValue(trace.join(""), "ab");
trace = [];
assert.sameValue(order(6n + 1n, trace), "c", "BigInt selector");
assert.sameValue(trace.join(""), "abc");
trace = [];
assert.sameValue(order(o1, trace), "e", "object selector");
assert.sameValue(trace.join(""), "abce");
trace = [];
assert.sameValue(order({}, trace), "dc", "default then fall-through into the next case");
assert.sameValue(trace.join(""), "abce", "all selectors evaluated before default");

// A throwing selector leaves the switch with the thrown value, later
// selectors are not evaluated.
function throwingSelector(v) {
  var seen = [];
  try {
    switch (v) {
      case (seen.push(1), "no"): return "no";
      case (function () { seen.push(2); throw new RangeError("boom"); })(): return "unreached";
      case (seen.push(3), v): return "unreached too";
    }
  } catch (e) {
    return e.constructor.name + ":" + seen.join(",");
  }
}
assert.sameValue(throwingSelector({}), "RangeError:1,2", "throwing selector");
assert.sameValue(throwingSelector("no"), "no", "match before the throwing selector");

// A discriminant that is an object is compared by identity and survives GC
// while the case tests allocate.
function objectDiscriminant(n) {
  var target = { n: n };
  var other = { n: n };
  var hits = 0;
  for (var i = 0; i < 40; i++) {
    switch (target) {
      case (junk(), other): hits += 100; break;
      case (junk(), { n: n }): hits += 1000; break;
      case (junk(), target): hits += 1; break;
    }
    if (i % 8 === 0) $262.gc();
  }
  return hits;
}
function junk() {
  var a = [];
  for (var i = 0; i < 50; i++) a.push({ i: i, s: "x" + i });
  return a;
}
assert.sameValue(objectDiscriminant(1), 40, "object discriminant under allocation pressure");

function objectComparisonsUnderGc() {
  var live = [];
  for (var i = 0; i < 30; i++) live.push({ i: i });
  var hits = 0;
  for (var j = 0; j < 30; j++) {
    if (live[j] === (junk(), live[j])) hits++;
    if (live[j] !== (junk(), live[(j + 1) % 30])) hits++;
    if (j % 10 === 0) $262.gc();
  }
  return hits;
}
assert.sameValue(objectComparisonsUnderGc(), 60, "object === with allocating right operand");

// Comparisons in other expression positions.
function positions(a, b) {
  var r = [];
  r.push(a === b ? "t" : "f");
  r.push((a === b) === true ? "t" : "f");
  r.push(!(a !== b) ? "t" : "f");
  r.push((a === b) && "and");
  r.push((a !== b) || "or");
  for (var i = 0; i === 0 && a === b; i++) r.push("loop");
  do { r.push("do"); } while (a !== a);
  return r.join(",");
}
assert.sameValue(positions(s1, owned[0]), "t,t,t,and,or,loop,do");
assert.sameValue(positions(s1, s2), "f,f,f,false,true,do");
assert.sameValue(positions("ab", "a" + "b"), "t,t,t,and,or,loop,do");
