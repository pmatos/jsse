/*---
description: >
  for-in inside a function body enumerates a snapshot of the keys, skips keys
  deleted before they are visited, supports every head form (including
  destructuring assignment targets), and leaves its enumerator without
  observable side effects on break, continue, return and throw.
info: |
  jsse issue #960: the VM keeps the for-in enumerator (the same
  %ForInIteratorPrototype% object the generator state machines use) as an
  operand-stack slot; the head binding reuses the tree-walker's binding code.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
includes: [compareArray.js]
---*/

function keys() {
  var out = [];
  var o = { b: 1, a: 2, 1: "x", 0: "y" };
  for (var k in o) out.push(k);
  return out;
}
assert.compareArray(keys(), ["0", "1", "b", "a"], "integer keys first, then insertion order");

function inheritedAndShadowed() {
  var proto = { p: 1, shadowed: 1 };
  var o = Object.create(proto);
  o.own = 1;
  Object.defineProperty(o, "shadowed", { value: 2, enumerable: false });
  var out = [];
  for (var k in o) out.push(k);
  return out;
}
assert.compareArray(
  inheritedAndShadowed(),
  ["own", "p"],
  "inherited enumerable keys, shadowed by a non-enumerable own one, are skipped"
);

function deletedBeforeVisitedAreSkipped() {
  var o = { a: 1, b: 2, c: 3 };
  var out = [];
  for (var k in o) { out.push(k); delete o.b; delete o.c; }
  return out;
}
assert.compareArray(deletedBeforeVisitedAreSkipped(), ["a"], "deleted keys are not visited");

function addedKeysAreNotVisited() {
  var o = { a: 1 };
  var out = [];
  for (var k in o) { out.push(k); o.added = 1; }
  return out;
}
assert.compareArray(addedKeysAreNotVisited(), ["a"], "keys added during enumeration are not visited");

function nullishIsEmpty() {
  var count = 0;
  for (var k in null) count++;
  for (var k in undefined) count++;
  return count;
}
assert.sameValue(nullishIsEmpty(), 0, "null and undefined enumerate nothing");

function primitivesAreWrapped() {
  var out = [];
  for (var k in "ab") out.push(k);
  for (var k in 5) out.push(k);
  return out;
}
assert.compareArray(primitivesAreWrapped(), ["0", "1"], "string indices, no keys for a number");

function headForms() {
  var out = [];
  var o = { p: 1 };
  var x, y;
  var target = {};
  for (var a in o) out.push(a);
  for (let b in o) out.push(b);
  for (const c in o) out.push(c);
  for (x in o) out.push(x);
  for (target.k in o) out.push(target.k);
  for ([x, y] in { ab: 1 }) out.push(x, y);
  for ({ length: y } in { abc: 1 }) out.push(y);
  for (var [m, n] in { zq: 1 }) out.push(m, n);
  return out;
}
assert.compareArray(
  headForms(),
  ["p", "p", "p", "p", "p", "a", "b", 3, "z", "q"],
  "head binding forms"
);

function destructuringAssignmentUpdatesExistingBindings() {
  var x = "old", y = "old";
  for ([x, y] in { cd: 1 }) {}
  return [x, y];
}
assert.compareArray(
  destructuringAssignmentUpdatesExistingBindings(),
  ["c", "d"],
  "a destructuring assignment target assigns, it does not declare"
);

function closuresCaptureEachKey() {
  var fns = [];
  for (let k in { a: 1, b: 2 }) fns.push(function () { return k; });
  return fns.map(function (f) { return f(); });
}
assert.compareArray(closuresCaptureEachKey(), ["a", "b"], "per-iteration binding");

function breakContinueReturn() {
  var out = [];
  var o = { a: 1, b: 2, c: 3, d: 4 };
  for (var k in o) { if (k === "a") continue; if (k === "c") break; out.push(k); }
  function f() { for (var k in o) { if (k === "b") return k; } }
  out.push(f());
  outer: for (var i = 0; i < 2; i++) {
    for (var k in o) { if (k === "b") continue outer; out.push(i + k); }
  }
  return out;
}
assert.compareArray(
  breakContinueReturn(),
  ["b", "b", "0a", "1a"],
  "break, continue, return and labelled continue"
);

function throwFromBody() {
  var log = [];
  for (var round = 0; round < 2; round++) {
    try {
      for (var k in { a: 1, b: 2 }) { log.push(k); throw "t" + round; }
    } catch (e) { log.push(e); }
  }
  return log;
}
assert.compareArray(throwFromBody(), ["a", "t0", "a", "t1"], "throw out of a loop, repeated");

function proxyEnumeration() {
  var log = [];
  var p = new Proxy({ a: 1, b: 2 }, {
    ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { log.push("gopd " + k); return Reflect.getOwnPropertyDescriptor(t, k); },
    has(t, k) { log.push("has " + k); return Reflect.has(t, k); },
  });
  var out = [];
  for (var k in p) out.push(k);
  return out.concat(log.filter(function (s) { return s.indexOf("ownKeys") === 0; }));
}
assert.compareArray(proxyEnumeration(), ["a", "b", "ownKeys"], "a proxy is enumerated through its traps");

function enumeratorIsNotObservable() {
  Object.defineProperty(Object.prototype, "return", {
    value: function () { throw new Test262Error("return must not be looked up"); },
    configurable: true,
  });
  Object.defineProperty(Object.prototype, "next", {
    value: function () { throw new Test262Error("next must not be looked up"); },
    configurable: true,
  });
  try {
    var out = [];
    for (var k in { a: 1, b: 2 }) { out.push(k); if (k === "a") break; }
    for (var k in { c: 1 }) out.push(k);
    return out;
  } finally {
    delete Object.prototype.return;
    delete Object.prototype.next;
  }
}
assert.compareArray(enumeratorIsNotObservable(), ["a", "c"], "no iterator protocol lookups");
