/*---
description: >
  A for-in statement whose body suspends a generator still enumerates the
  object's properties and binds each key to the loop head.
esid: sec-runtime-semantics-forinofloopevaluation
info: |
  ForIn/OfBodyEvaluation repeatedly steps the enumerator created by
  EnumerateObjectProperties, assigns each key to the loop head and evaluates
  the body. A `yield` in the body suspends the generator (27.5.3.3
  GeneratorYield) and resuming continues the enumeration where it stopped.
includes: [compareArray.js]
features: [generators]
---*/

function* varHead(o) {
  for (var k in o) {
    yield k;
  }
  yield 'after';
}
assert.compareArray([...varHead({ a: 1, b: 2 })], ['a', 'b', 'after'], 'var head');

function* outerVarHead(o) {
  var k;
  for (k in o) yield k;
  yield k;
}
assert.compareArray([...outerVarHead({ x: 1, y: 2 })], ['x', 'y', 'y'], 'assignment to existing binding');

function* memberTarget(o, target) {
  for (target.p in o) yield target.p;
}
var holder = {};
assert.compareArray([...memberTarget({ a: 1, b: 2 }, holder)], ['a', 'b'], 'member expression target');
assert.sameValue(holder.p, 'b', 'member expression target keeps the last key');

function* indexTarget(o, arr) {
  for (arr[0] in o) yield arr[0];
}
var list = [];
assert.compareArray([...indexTarget({ a: 1, b: 2 }, list)], ['a', 'b'], 'computed member target');

function* proto() {
  var parent = { shared: 1, inherited: 2 };
  var child = Object.create(parent);
  child.own = 3;
  child.shared = 4;
  Object.defineProperty(child, 'hidden', { value: 5, enumerable: false });
  child[Symbol('s')] = 6;
  for (var k in child) yield k;
}
assert.compareArray(
  [...proto()],
  ['own', 'shared', 'inherited'],
  'own keys first, shadowed and non-enumerable keys skipped, symbols excluded'
);

function* primitive() {
  for (var i in 'ab') yield i;
}
assert.compareArray([...primitive()], ['0', '1'], 'string primitive is wrapped with ToObject');

function* interleaved(o) {
  var seen = [];
  for (var k in o) {
    seen.push(k);
    yield seen.slice();
  }
}
var it = interleaved({ p: 1, q: 2, r: 3 });
assert.compareArray(it.next().value, ['p'], 'first step');
assert.compareArray(it.next().value, ['p', 'q'], 'second step resumes at the same position');
assert.compareArray(it.next().value, ['p', 'q', 'r'], 'third step');
assert.sameValue(it.next().done, true, 'enumeration ends');

function* strictUndeclared(o) {
  'use strict';
  for (undeclaredForInTarget in o) yield 1;
}
var strictIt = strictUndeclared({ a: 1 });
assert.throws(ReferenceError, function () {
  strictIt.next();
}, 'assignment to an undeclared identifier throws in strict mode');
