/*---
description: >
  switch statements and function-top-level let/const compiled to bytecode
  follow selector evaluation order, strict equality, fall-through, default
  placement, break/continue targets, temporal dead zone and const
  immutability.
info: |
  jsse issue #902: switch and function-top-level lexical declarations now
  have native lowerings. This file pins their observable semantics, which
  must match the tree-walker exactly.

  The switch Statement (14.12.4), CaseBlock evaluation (14.12.2),
  CaseClauseIsSelected (14.12.3), IsStrictlyEqual (7.2.14), let and const
  declarations (14.3.1.2), InitializeBinding (9.1.1.1.4).
esid: sec-switch-statement-runtime-semantics-evaluation
---*/

function selectorOrderAndFallthrough(n) {
  var log = [];
  var r = [];
  switch (n) {
    case (log.push("s1"), 1):
      r.push("one");
    case (log.push("s2"), 2):
      r.push("two");
      break;
    default:
      r.push("default");
    case (log.push("s3"), 3):
      r.push("three");
      break;
    case (log.push("s4"), 4):
      r.push("four");
  }
  return r.join("+") + "|" + log.join("");
}
assert.sameValue(selectorOrderAndFallthrough(1), "one+two|s1");
assert.sameValue(selectorOrderAndFallthrough(2), "two|s1s2");
assert.sameValue(selectorOrderAndFallthrough(3), "three|s1s2s3");
assert.sameValue(selectorOrderAndFallthrough(4), "four|s1s2s3s4");
assert.sameValue(selectorOrderAndFallthrough(9), "default+three|s1s2s3s4");

function strictEqualitySelection(v) {
  switch (v) {
    case "1": return "string";
    case 1: return "number";
    case NaN: return "nan";
    case null: return "null";
    case undefined: return "undefined";
    default: return "none";
  }
}
assert.sameValue(strictEqualitySelection("1"), "string");
assert.sameValue(strictEqualitySelection(1), "number");
assert.sameValue(strictEqualitySelection(NaN), "none");
assert.sameValue(strictEqualitySelection(null), "null");
assert.sameValue(strictEqualitySelection(undefined), "undefined");
assert.sameValue(strictEqualitySelection(true), "none");

function discriminantEvaluatedOnce() {
  var calls = 0;
  function d() { calls++; return 2; }
  var hits = [];
  switch (d()) {
    case 1: hits.push(1); break;
    case 2: hits.push(2); break;
    case 3: hits.push(3); break;
  }
  return calls + ":" + hits.join();
}
assert.sameValue(discriminantEvaluatedOnce(), "1:2");

function emptySwitchAndNoMatch() {
  var n = 0;
  switch (n) {}
  switch (n) { case 1: n = 5; }
  return n;
}
assert.sameValue(emptySwitchAndNoMatch(), 0);

function breakAndContinueTargets() {
  var out = [];
  outer: for (var i = 0; i < 5; i++) {
    switch (i) {
      case 0:
        continue;
      case 1:
        out.push("a");
        break;
      case 2:
        out.push("b");
        continue outer;
      case 3:
        try { out.push("t"); continue; } finally { out.push("f"); }
      default:
        break outer;
    }
    out.push("after" + i);
  }
  return out.join(",");
}
assert.sameValue(breakAndContinueTargets(), "a,after1,b,t,f");

function labeledSwitchBreak() {
  var out = [];
  lbl: switch (1) {
    case 1:
      out.push("x");
      break lbl;
      out.push("unreachable");
  }
  out.push("end");
  return out.join(",");
}
assert.sameValue(labeledSwitchBreak(), "x,end");

function nestedSwitchBreakOnlyLeavesInner() {
  var out = [];
  switch (1) {
    case 1:
      switch (2) {
        case 2:
          out.push("inner");
          break;
      }
      out.push("outer");
  }
  return out.join(",");
}
assert.sameValue(nestedSwitchBreakOnlyLeavesInner(), "inner,outer");

function returnFromCase(x) {
  switch (x) {
    case 1: return "r1";
    case 2: { return "r2"; }
  }
  return "fell";
}
assert.sameValue(returnFromCase(1), "r1");
assert.sameValue(returnFromCase(2), "r2");
assert.sameValue(returnFromCase(3), "fell");

function throwFromSelector() {
  switch (1) {
    case (function () { throw new RangeError("selector"); })():
      return "no";
  }
}
assert.throws(RangeError, throwFromSelector);

function caseBodyWithLexicalDeclaration(x) {
  switch (x) {
    case 1:
      let y = "one";
      return y;
    case 2:
      function inCase() { return "fn"; }
      return inCase();
  }
  return "none";
}
assert.sameValue(caseBodyWithLexicalDeclaration(1), "one");
assert.sameValue(caseBodyWithLexicalDeclaration(2), "fn");
assert.sameValue(caseBodyWithLexicalDeclaration(3), "none");

function topLevelLexical() {
  const a = 3;
  let b = a * 2;
  let c;
  var out = [typeof c, a, b];
  b = b + 1;
  out.push(b);
  return out.join();
}
assert.sameValue(topLevelLexical(), "undefined,3,6,7");

function constIsImmutable() {
  const k = 1;
  try {
    k = 2;
  } catch (e) {
    return e.constructor === TypeError && k === 1;
  }
  return false;
}
assert.sameValue(constIsImmutable(), true);

function temporalDeadZone() {
  var seen = [];
  try { seen.push(x); } catch (e) { seen.push(e.name); }
  try { x = 5; } catch (e) { seen.push(e.name); }
  try { seen.push(typeof x); } catch (e) { seen.push(e.name); }
  let x = 1;
  seen.push(x);
  return seen.join();
}
assert.sameValue(temporalDeadZone(), "ReferenceError,ReferenceError,ReferenceError,1");

function closureCapturesLexical() {
  let counter = 0;
  function bump() { counter++; return counter; }
  bump();
  bump();
  return counter;
}
assert.sameValue(closureCapturesLexical(), 2);

function lexicalInitializerThrows() {
  try {
    let boom = (function () { throw new SyntaxError("init"); })();
    return "unreachable";
  } catch (e) {
    return e.name;
  }
}
assert.sameValue(lexicalInitializerThrows(), "SyntaxError");

function hotLoopWithLexicals(n) {
  const step = 3;
  let total = 0;
  for (var i = 0; i < n; i++) {
    switch (i & 3) {
      case 0: total += step; break;
      case 1: total += 1;
      case 2: total += 2; break;
      default: total -= 1;
    }
  }
  return total;
}
assert.sameValue(hotLoopWithLexicals(8), 22);
