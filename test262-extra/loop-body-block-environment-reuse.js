/*---
description: A loop body block's let/const bindings behave as a fresh binding per iteration whether or not the body shares one environment for the whole loop
info: |
  14.7.4.2 Runtime Semantics: ForBodyEvaluation
  14.7.5.7 Runtime Semantics: ForIn/OfBodyEvaluation
  14.2.2 Runtime Semantics: Evaluation of Block (BlockDeclarationInstantiation)
  9.1.1.1.1 HasBinding / 9.1.1.1.6 GetBindingValue (TDZ ReferenceError)
  14.7.4.4 Runtime Semantics: CreatePerIterationEnvironment

  Every evaluation of a Block creates a new Environment Record whose let/const
  bindings start uninitialized. A body that no closure or eval can observe may
  keep one environment across iterations, but a read before the declaration, a
  declaration without an initializer, a shadowed name in the loop head, a
  closure created in the body, a labelled continue, a generator suspension and a
  thrown error must all behave as with a fresh environment.
esid: sec-runtime-semantics-forbodyevaluation
flags: [noStrict]
includes: [compareArray.js]
---*/

function plainFor() {
  var s = 0;
  for (let i = 0; i < 5; i++) {
    let t = i + 1;
    const u = t * 2;
    s = s + u;
  }
  return s;
}
assert.sameValue(plainFor(), 30, "for (let ...) with a body let/const");

function uninitializedIsUndefined() {
  var seen = [];
  for (let i = 0; i < 4; i++) {
    let u;
    seen.push(u);
    u = i;
  }
  return seen;
}
assert.compareArray(
  uninitializedIsUndefined(),
  [undefined, undefined, undefined, undefined],
  "let without an initializer starts at undefined each iteration"
);

function tdzReadBeforeDeclaration() {
  var log = [];
  for (let i = 0; i < 3; i++) {
    try {
      log.push(typeof t);
    } catch (e) {
      log.push(e.constructor === ReferenceError ? "TDZ" : "other");
    }
    let t = i;
    log.push(t);
  }
  return log;
}
assert.compareArray(
  tdzReadBeforeDeclaration(),
  ["TDZ", 0, "TDZ", 1, "TDZ", 2],
  "a read before the declaration throws on every iteration"
);

function selfReferencingInitializer() {
  var errors = 0;
  for (let i = 0; i < 3; i++) {
    try {
      let t = t + 1;
    } catch (e) {
      if (e.constructor === ReferenceError) errors++;
    }
  }
  return errors;
}
assert.sameValue(selfReferencingInitializer(), 3, "let t = t throws every iteration");

function selfReferenceInSecondDeclarator() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    let a = i, b = a + 10;
    out.push(b);
  }
  return out;
}
assert.compareArray(selfReferenceInSecondDeclarator(), [10, 11, 12], "a later declarator reads an earlier one");

function closureCapturesEachIteration() {
  var fs = [];
  for (let i = 0; i < 3; i++) {
    let t = i * 10;
    fs.push(function () { return t + i; });
    t++;
  }
  return fs.map(function (f) { return f(); });
}
assert.compareArray(closureCapturesEachIteration(), [1, 12, 23], "closures keep their own iteration's bindings");

function arrowInBodyOfWhile() {
  var fs = [], i = 0;
  while (i < 3) {
    let t = i;
    fs.push(() => t);
    i++;
  }
  return fs.map(function (f) { return f(); });
}
assert.compareArray(arrowInBodyOfWhile(), [0, 1, 2], "while body with a capturing arrow");

function closureInTestAndUpdate() {
  var fs = [];
  for (let i = 0; fs.push(() => i) < 3; i++) {
    let t = i;
    t = t + 1;
  }
  return fs.map(function (f) { return f(); });
}
assert.compareArray(closureInTestAndUpdate(), [0, 1, 2], "closures in the head see per-iteration copies");

function testReadsOuterNameShadowedByBody() {
  var t = 2, runs = 0;
  for (let i = 0; i < 5 && t > 0; i++) {
    let t = -1;
    runs++;
  }
  return [runs, t];
}
assert.compareArray(testReadsOuterNameShadowedByBody(), [5, 2], "the test expression reads the outer binding");

function updateReadsOuterNameShadowedByBody() {
  var step = 2, seen = [];
  for (var i = 0; i < 6; i += step) {
    let step = 100;
    seen.push(i + step);
  }
  return seen;
}
assert.compareArray(updateReadsOuterNameShadowedByBody(), [100, 102, 104], "the update expression reads the outer binding");

function whileTestReadsOuterName() {
  var n = 3, count = 0;
  while (n > 0) {
    let m = n;
    count++;
    n = m - 1;
  }
  var k = 2, runs = 0;
  while (k-- > 0) {
    let k = 99;
    runs++;
  }
  return [count, runs];
}
assert.compareArray(whileTestReadsOuterName(), [3, 2], "while test and body use distinct names");

function doWhileTestReadsOuterName() {
  var t = 1, runs = 0;
  do {
    let t = 0;
    runs++;
  } while (t-- > -2);
  return [runs, t];
}
assert.compareArray(doWhileTestReadsOuterName(), [4, -3], "do-while test reads the outer binding");

function headNameShadowedInBodyOfForOf() {
  var out = [];
  for (const x of [1, 2, 3]) {
    let y = x * 2;
    out.push(y);
  }
  for (const x of [1, 2]) {
    let x = 7;
    out.push(x);
  }
  return out;
}
assert.compareArray(headNameShadowedInBodyOfForOf(), [2, 4, 6, 7, 7], "for-of head and body bindings");

function forInBody() {
  var keys = [];
  for (var k in { a: 1, b: 2 }) {
    const upper = k.toUpperCase();
    keys.push(upper);
  }
  return keys;
}
assert.compareArray(forInBody(), ["A", "B"], "for-in with a body const");

function forOfVarAndAssignHeads() {
  var v, out = [];
  for (v of [1, 2]) {
    let w = v + 1;
    out.push(w);
  }
  for (var u of [3, 4]) {
    let w = u + 1;
    out.push(w);
  }
  return out;
}
assert.compareArray(forOfVarAndAssignHeads(), [2, 3, 4, 5], "for-of with var and assignment heads");

function objectPatternDeclaration() {
  var src = [{ a: 1, b: 2 }, { a: 3, b: 4 }], out = [];
  for (let i = 0; i < src.length; i++) {
    const { a, b } = src[i];
    out.push(a * b);
  }
  return out;
}
assert.compareArray(objectPatternDeclaration(), [2, 12], "object pattern declaration in the body");

function arrayPatternDeclaration() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    const [p, q] = [i, i + 1];
    out.push(p + q);
  }
  return out;
}
assert.compareArray(arrayPatternDeclaration(), [1, 3, 5], "array pattern declaration in the body");

function constIsReinitialisedNotReassignable() {
  var seen = [];
  for (let i = 0; i < 3; i++) {
    const c = i;
    try {
      c = 99;
    } catch (e) {
      seen.push(e.constructor === TypeError ? c : "other");
    }
  }
  return seen;
}
assert.compareArray(constIsReinitialisedNotReassignable(), [0, 1, 2], "assigning a body const throws each iteration");

function labelledContinueAndBreak() {
  var out = [];
  outer: for (let i = 0; i < 4; i++) {
    let a = i * 10;
    for (let j = 0; j < 4; j++) {
      let b = a + j;
      if (j === 1) continue outer;
      if (i === 3) break outer;
      out.push(b);
    }
  }
  return out;
}
assert.compareArray(labelledContinueAndBreak(), [0, 10, 20], "labelled continue/break across merged scopes");

function breakAndContinueInBody() {
  var out = [];
  for (let i = 0; i < 6; i++) {
    let t = i;
    if (t === 1) continue;
    if (t === 4) break;
    out.push(t);
  }
  var w = 0;
  while (true) {
    let t = w++;
    if (t > 2) break;
    out.push(t);
  }
  return out;
}
assert.compareArray(breakAndContinueInBody(), [0, 2, 3, 0, 1, 2], "break/continue in merged bodies");

function returnFromBody() {
  for (let i = 0; i < 3; i++) {
    let t = i * 5;
    if (i === 2) return t;
  }
  return -1;
}
assert.sameValue(returnFromBody(), 10, "return from inside a merged body");

function tryCatchInBody() {
  var out = [];
  for (let i = 0; i < 4; i++) {
    let t = i;
    try {
      if (t % 2) throw t;
      out.push("ok" + t);
    } catch (e) {
      out.push("caught" + e + t);
    } finally {
      out.push("f" + t);
    }
  }
  return out;
}
assert.compareArray(
  tryCatchInBody(),
  ["ok0", "f0", "caught11", "f1", "ok2", "f2", "caught33", "f3"],
  "try/catch/finally inside a merged body"
);

function throwThenReenterLoop() {
  var out = [];
  for (var round = 0; round < 2; round++) {
    try {
      for (let i = 0; i < 3; i++) {
        let t = i;
        if (round === 0 && i === 1) throw t;
        out.push(t);
      }
    } catch (e) {
      out.push("e" + e);
    }
  }
  return out;
}
assert.compareArray(throwThenReenterLoop(), [0, "e1", 0, 1, 2], "re-entering a loop starts new TDZ bindings");

function switchInBody() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    let t = i;
    switch (t) {
      case 1: {
        let t2 = t + 10;
        out.push(t2);
        break;
      }
      default:
        out.push(t);
    }
  }
  return out;
}
assert.compareArray(switchInBody(), [0, 11, 2], "switch inside a merged body");

function nestedLoopsEachWithLets() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    let a = i;
    for (let j = 0; j < 2; j++) {
      let b = a * 10 + j;
      out.push(b);
    }
    let c = a + 100;
    out.push(c);
  }
  return out;
}
assert.compareArray(
  nestedLoopsEachWithLets(),
  [0, 1, 100, 10, 11, 101, 20, 21, 102],
  "nested loops, each body with its own lets"
);

function directEvalInBody() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    let t = i;
    eval("var leaked" + i + " = t + 1");
    out.push(eval("t * 2"));
  }
  return out.concat([leaked0, leaked1, leaked2]);
}
assert.compareArray(directEvalInBody(), [0, 2, 4, 1, 2, 3], "direct eval sees the iteration's binding and declares vars");

function directEvalClosureInBody() {
  var fs = [];
  for (let i = 0; i < 3; i++) {
    let t = i;
    fs.push(eval("(function () { return t; })"));
  }
  return fs.map(function (f) { return f(); });
}
assert.compareArray(directEvalClosureInBody(), [0, 1, 2], "a closure made by eval captures its iteration");

function withInBody() {
  var obj = { t: "obj" }, out = [];
  for (let i = 0; i < 2; i++) {
    let t = "let" + i;
    with (obj) {
      out.push(t);
    }
  }
  return out;
}
assert.compareArray(withInBody(), ["obj", "obj"], "with shadows the body binding");

function classInBody() {
  var out = [];
  for (let i = 0; i < 2; i++) {
    let t = i;
    class C { static get() { return t; } }
    out.push(C.get());
  }
  return out;
}
assert.compareArray(classInBody(), [0, 1], "a class in the body captures its iteration");

function blockFunctionInBody() {
  var out = [];
  for (let i = 0; i < 2; i++) {
    let t = i;
    function g() { return t; }
    out.push(g());
  }
  return out;
}
assert.compareArray(blockFunctionInBody(), [0, 1], "a function declaration in the body captures its iteration");

function* generatorLoop() {
  for (let i = 0; i < 3; i++) {
    let t = i * 2;
    const got = yield t;
    let u = t + (got || 0);
    yield u;
  }
}
var g = generatorLoop();
assert.sameValue(g.next().value, 0, "first yield");
assert.sameValue(g.next(5).value, 5, "binding survives the suspension");
assert.sameValue(g.next().value, 2, "second iteration");
assert.sameValue(g.next(1).value, 3, "second iteration after resume");
assert.sameValue(g.next().value, 4, "third iteration");
assert.sameValue(g.next().value, 4, "third iteration, second yield");
assert.sameValue(g.next().done, true, "finished");

function* interleavedGenerators() {
  for (const x of [1, 2]) {
    let y = x * 100;
    yield y;
    yield y + 1;
  }
}
var g1 = interleavedGenerators(), g2 = interleavedGenerators();
var order = [g1.next().value, g2.next().value, g1.next().value, g2.next().value, g1.next().value, g2.next().value];
assert.compareArray(order, [100, 100, 101, 101, 200, 200], "two live generators keep separate loop environments");

function shadowsFunctionLevelNames() {
  var t = "fn", out = [];
  for (let i = 0; i < 2; i++) {
    out.push(t);
    {
      let t = "inner";
      out.push(t);
    }
  }
  for (let i = 0; i < 2; i++) {
    let s = i;
    out.push(typeof t);
  }
  return out;
}
assert.compareArray(
  shadowsFunctionLevelNames(),
  ["fn", "inner", "fn", "inner", "string", "string"],
  "an outer binding read before an inner block's declaration is not shadowed"
);

function readsOuterBeforeSameNameDeclaration() {
  var t = "outer", out = [];
  for (let i = 0; i < 2; i++) {
    try {
      out.push(t);
    } catch (e) {
      out.push(e.constructor === ReferenceError ? "TDZ" : "other");
    }
    let t = "inner";
  }
  return out;
}
assert.compareArray(readsOuterBeforeSameNameDeclaration(), ["TDZ", "TDZ"], "the body binding shadows from the block's start");

function manyNames() {
  var s = 0;
  for (let i = 0; i < 3; i++) {
    let a = i, b = a + 1, c = b + 1, d = c + 1;
    const e = a + b + c + d;
    s += e;
  }
  return s;
}
assert.sameValue(manyNames(), 6 + 10 + 14, "several body bindings");

function ranWithGcBetweenIterations() {
  var out = [];
  for (let i = 0; i < 3; i++) {
    let o = { v: i };
    if (typeof $262 !== "undefined" && $262.gc) $262.gc();
    out.push(o.v);
  }
  return out;
}
assert.compareArray(ranWithGcBetweenIterations(), [0, 1, 2], "body objects survive collections");

var tdzForms = {
  shorthand: function () { var r = []; for (let i = 0; i < 3; i++) { try { ({ t }); r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  assign: function () { var r = []; for (let i = 0; i < 3; i++) { try { t = 5; r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  update: function () { var r = []; for (let i = 0; i < 3; i++) { try { t++; r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  compound: function () { var r = []; for (let i = 0; i < 3; i++) { try { t += 1; r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  destructuringAssignment: function () { var r = []; for (let i = 0; i < 3; i++) { try { [t] = [1]; r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  objectDestructuringAssignment: function () { var r = []; for (let i = 0; i < 3; i++) { try { ({ t } = { t: 1 }); r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  template: function () { var r = []; for (let i = 0; i < 3; i++) { try { `${t}`; r.push("no"); } catch (e) { r.push(e.name); } let t = i; } return r.join(); },
  nestedBlock: function () { var r = []; for (let i = 0; i < 3; i++) { { try { t; r.push("no"); } catch (e) { r.push(e.name); } } let t = i; } return r.join(); },
  switchCase: function () { var r = []; for (let i = 0; i < 3; i++) { switch (0) { case 0: try { t; r.push("no"); } catch (e) { r.push(e.name); } } let t = i; } return r.join(); }
};
for (var tdzName in tdzForms) {
  assert.sameValue(
    tdzForms[tdzName](),
    "ReferenceError,ReferenceError,ReferenceError",
    "TDZ read through " + tdzName + " throws on every iteration"
  );
}
