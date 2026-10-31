// Copyright (C) 2026 the JSSE authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
description: >
  A switch whose case selectors are all number or string literals picks its
  clause exactly as the linear CaseClauseIsSelected chain does: strict
  equality (no coercion, +0 equal to -0, NaN never selected, a string never
  equal to a number or a BigInt), the first of several equal selectors wins,
  execution falls through the clauses that follow, the default clause runs
  when nothing matches wherever it is placed, and the discriminant is
  evaluated once. Small dense, wide sparse and string tables are covered, as
  are break and continue out of the clauses, lexical declarations in the
  clauses, and a generator.
info: |
  jsse issue #1491: the bytecode compiler dispatches such a switch through a
  per-chunk table instead of comparing against each selector in turn. The
  result must be that of CaseBlockEvaluation (14.12.4), IsStrictlyEqual
  (7.2.15) and the switch statement evaluation (14.12.3).
esid: sec-runtime-semantics-caseblockevaluation
features: [BigInt, Symbol, generators]
---*/

function classify(x) {
  var r = "";
  switch (x) {
    case 1: r += "one";
    case 2: r += "two"; break;
    default: r += "d";
    case 3: r += "three"; break;
    case "a": r += "A"; break;
    case 1: r += "dup"; break;
    case 4: r += "four";
  }
  return r;
}
assert.sameValue(classify(1), "onetwo", "first of two equal selectors, then fall-through");
assert.sameValue(classify(2), "two", "break stops the fall-through");
assert.sameValue(classify(3), "three", "clause after the default clause");
assert.sameValue(classify("a"), "A", "string selector");
assert.sameValue(classify(4), "four", "last clause falls off the end");
assert.sameValue(classify(5), "dthree", "default in the middle falls through to the next clause");
assert.sameValue(classify("1"), "dthree", "string is not strictly equal to a number");
assert.sameValue(classify(1n), "dthree", "BigInt is not strictly equal to a number");
assert.sameValue(classify(true), "dthree", "boolean is not strictly equal to a number");
assert.sameValue(classify(null), "dthree", "null selects the default clause");
assert.sameValue(classify(undefined), "dthree", "undefined selects the default clause");
assert.sameValue(classify(NaN), "dthree", "NaN is never selected");
assert.sameValue(classify(Symbol.iterator), "dthree", "symbol selects the default clause");
assert.sameValue(classify({}), "dthree", "object selects the default clause");
assert.sameValue(classify(1.5), "dthree", "fraction between integer selectors");
assert.sameValue(classify("A"), "dthree", "string selectors are case sensitive");
assert.sameValue(classify("aa"), "dthree", "string selector prefix");

function defaultPlacement(x) {
  var first = "", middle = "", last = "", none;
  switch (x) { default: first = "d"; case 1: first += "1"; break; case 2: first += "2"; break; case 3: first += "3"; }
  switch (x) { case 1: middle = "1"; break; default: middle = "d"; case 2: middle += "2"; break; case 3: middle = "3"; }
  switch (x) { case 1: last = "1"; break; case 2: last = "2"; break; case 3: last = "3"; break; default: last = "d"; }
  none = "n";
  switch (x) { case 1: none = "1"; break; case 2: none = "2"; break; case 3: none = "3"; }
  return [first, middle, last, none].join("|");
}
assert.sameValue(defaultPlacement(1), "1|1|1|1");
assert.sameValue(defaultPlacement(2), "2|2|2|2");
assert.sameValue(defaultPlacement(3), "3|3|3|3");
assert.sameValue(defaultPlacement(9), "d1|d2|d|n", "default clause first, middle, last, absent");

function zeros(x) {
  switch (x) {
    case -0: return "zero";
    case 1: return "one";
    case 2: return "two";
    default: return "other";
  }
}
function positiveZero(x) {
  switch (x) {
    case 0: return "zero";
    case 1: return "one";
    case 2: return "two";
    default: return "other";
  }
}
assert.sameValue(zeros(0), "zero", "+0 against case -0");
assert.sameValue(zeros(-0), "zero", "-0 against case -0");
assert.sameValue(positiveZero(-0), "zero", "-0 against case 0");
assert.sameValue(positiveZero(0), "zero", "+0 against case 0");
assert.sameValue(zeros(NaN), "other", "NaN against case -0");
assert.sameValue(zeros(""), "other", "empty string is not 0");
assert.sameValue(zeros("0"), "other", "'0' is not 0");
assert.sameValue(zeros(false), "other", "false is not 0");
assert.sameValue(zeros(0n), "other", "0n is not 0");
assert.sameValue(zeros([]), "other", "[] is not 0");

function strings(x) {
  switch (x) {
    case "": return "empty";
    case "\uD800": return "lone high surrogate";
    case "\u{1F600}": return "emoji";
    case "ab": return "ab";
    case "a\0b": return "nul";
    case "0": return "string zero";
    case 0: return "number zero";
    default: return "other";
  }
}
assert.sameValue(strings(""), "empty");
assert.sameValue(strings(String("")), "empty");
assert.sameValue(strings("\uD800"), "lone high surrogate");
assert.sameValue(strings("\uDE00"), "other", "different lone surrogate");
assert.sameValue(strings("😀"), "emoji", "surrogate pair spelled out");
assert.sameValue(strings("a" + "b"), "ab", "string built at run time");
assert.sameValue(strings(["a", "b"].join("")), "ab", "joined string");
assert.sameValue(strings("xab".slice(1)), "ab", "sliced string");
assert.sameValue(strings("ab".concat("")), "ab");
assert.sameValue(strings("a\0b"), "nul");
assert.sameValue(strings("a"), "other");
assert.sameValue(strings("abc"), "other");
assert.sameValue(strings(0), "number zero", "number 0 against '0' and 0");
assert.sameValue(strings("0"), "string zero");
assert.sameValue(strings(-0), "number zero");
assert.sameValue(strings(new String("ab")), "other", "String object is not a primitive string");

// No coercion of the discriminant, and it is evaluated once.
var coerced = 0;
var tricky = {
  valueOf: function () { coerced++; return 1; },
  toString: function () { coerced++; return "ab"; },
  [Symbol.toPrimitive]: function () { coerced++; return 1; }
};
function noCoercion(x) {
  switch (x) {
    case 1: return "one";
    case 2: return "two";
    case "ab": return "ab";
    default: return "other";
  }
}
assert.sameValue(noCoercion(tricky), "other", "object with valueOf/toString/@@toPrimitive");
assert.sameValue(coerced, 0, "the discriminant is not coerced");

var evaluations = 0;
function discriminant() { evaluations++; return 3; }
function once() {
  switch (discriminant()) {
    case 1: return "one";
    case 2: return "two";
    case 3: return "three";
    case 4: return "four";
  }
}
assert.sameValue(once(), "three");
assert.sameValue(evaluations, 1, "discriminant evaluated once");

// Tables over wider and sparser keys.
function generated(keys) {
  var body = "switch (x) {";
  keys.forEach(function (k, i) {
    body += " case " + (typeof k === "string" ? JSON.stringify(k) : k) + ": return " + i + ";";
  });
  body += " default: return -1; }";
  return new Function("x", body);
}
var dense = [];
for (var i = 0; i < 300; i++) dense.push(i);
var denseSwitch = generated(dense);
for (var i = 0; i < 300; i++) assert.sameValue(denseSwitch(i), i, "dense " + i);
assert.sameValue(denseSwitch(300), -1, "dense, past the end");
assert.sameValue(denseSwitch(-1), -1, "dense, before the start");
assert.sameValue(denseSwitch(1.5), -1, "dense, fraction");
assert.sameValue(denseSwitch(-0), 0, "dense, -0");
assert.sameValue(denseSwitch("1"), -1, "dense, string");

var sparse = [0, 7, 1000, -5000, 2147483647, -2147483648, 4294967296, 1e300, 0.5, -2.25, 9007199254740992, 1e-300];
var sparseSwitch = generated(sparse);
sparse.forEach(function (k, i) {
  assert.sameValue(sparseSwitch(k), i, "sparse " + k);
  assert.sameValue(sparseSwitch(String(k)), -1, "sparse, string " + k);
});
assert.sameValue(sparseSwitch(1), -1);
assert.sameValue(sparseSwitch(2147483648), -1, "one past int32 max");
assert.sameValue(sparseSwitch(-2147483649), -1, "one below int32 min");
assert.sameValue(sparseSwitch(NaN), -1);
assert.sameValue(sparseSwitch(Infinity), -1);
assert.sameValue(sparseSwitch(-Infinity), -1);
assert.sameValue(sparseSwitch(-0), 0, "sparse, -0");

var duplicates = generated([5, 6, 5, 7, 6, "x", "x", 5, "y"]);
assert.sameValue(duplicates(5), 0, "first of three equal selectors");
assert.sameValue(duplicates(6), 1);
assert.sameValue(duplicates(7), 3);
assert.sameValue(duplicates("x"), 5, "first of two equal string selectors");
assert.sameValue(duplicates("y"), 8);

var negative = generated([-1, -2, -3, 0, 1]);
for (var i = 0; i < 5; i++) assert.sameValue(negative([-1, -2, -3, 0, 1][i]), i, "negative selector " + i);
assert.sameValue(negative(-4), -1);

var names = ["break", "case", "catch", "class", "const", "continue"];
var nameSwitch = generated(names);
names.forEach(function (s, i) { assert.sameValue(nameSwitch(s), i, "string " + s); });
assert.sameValue(nameSwitch("breaK"), -1);
assert.sameValue(nameSwitch(5), -1);

// break, continue, return and labels leave the clauses as before.
function loops() {
  var out = "";
  outer: for (var i = 0; i < 7; i++) {
    switch (i) {
      case 0: continue;
      case 1: out += "a"; break;
      case 2: out += "b"; continue outer;
      case 3:
        try { out += "t"; continue; } finally { out += "f"; }
      case 4: out += "c";
      case 5: out += "d"; break outer;
      default: out += "z";
    }
    out += "x" + i;
  }
  return out;
}
assert.sameValue(loops(), "ax1btfcd");

function nested(a, b) {
  switch (a) {
    case 1:
      switch (b) {
        case "x": return "1x";
        case "y": break;
        case "z": return "1z";
      }
      return "1-";
    case 2: return "2";
    case 3: return "3";
  }
  return "none";
}
assert.sameValue(nested(1, "x"), "1x");
assert.sameValue(nested(1, "y"), "1-", "break leaves only the inner switch");
assert.sameValue(nested(1, "q"), "1-");
assert.sameValue(nested(2, "x"), "2");
assert.sameValue(nested(9, "x"), "none");

// Lexical declarations in the clauses share one scope, entered before the dispatch.
function lexical(x) {
  switch (x) {
    case 1: let a = "one"; return a;
    case 2: return typeof a === "string" ? "bound" : "unbound";
    case 3: return a + "three";
    case 4: return (function () { return a; })();
    default: return "default";
  }
}
assert.sameValue(lexical(1), "one");
assert.sameValue(lexical(5), "default");
assert.throws(ReferenceError, function () { lexical(2); }, "typeof in the temporal dead zone");
assert.throws(ReferenceError, function () { lexical(3); }, "read in the temporal dead zone");
assert.throws(ReferenceError, function () { lexical(4); }, "closure read in the temporal dead zone");

function closures() {
  var fns = [];
  for (var i = 0; i < 4; i++) {
    switch (i) {
      case 0: fns.push(function () { return "zero"; }); break;
      case 1: fns.push(function () { return "one"; }); break;
      case 2: fns.push(function () { return "two"; }); break;
      default: fns.push(function () { return "many"; });
    }
  }
  return fns.map(function (f) { return f(); }).join(",");
}
assert.sameValue(closures(), "zero,one,two,many");

// A generator keeps its place inside the clause across a yield.
function* generator(x) {
  switch (x) {
    case 1: yield "a";
    case 2: yield "b"; break;
    case "s": yield "c"; return "done";
    default: yield "d";
  }
  yield "end";
}
function drain(it) {
  var out = [], step;
  while (!(step = it.next()).done) out.push(step.value);
  return out.join("") + (step.value === undefined ? "" : "/" + step.value);
}
assert.sameValue(drain(generator(1)), "abend");
assert.sameValue(drain(generator(2)), "bend");
assert.sameValue(drain(generator("s")), "c/done");
assert.sameValue(drain(generator(0)), "dend");

// Many calls: the same table is reused with different discriminants.
function lookup(x) {
  switch (x) {
    case 10: return 1;
    case 20: return 2;
    case 30: return 3;
    case "k": return 4;
    default: return 0;
  }
}
var sum = 0;
for (var i = 0; i < 200; i++) sum += lookup(10 * (i % 5)) + lookup(i % 2 ? "k" : "j");
assert.sameValue(sum, 640, "repeated dispatch");
