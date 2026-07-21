/*---
description: Function-call storage optimizations preserve activation and arguments semantics
info: |
  10.2.1.3 OrdinaryCallEvaluateBody
  10.2.11 FunctionDeclarationInstantiation
  10.4.4 Arguments Exotic Objects

  A fresh Function Environment Record is observed for every activation even
  when an implementation reuses backing storage. Simple identifier parameters
  are initialized in source order, escaped closures retain their activation,
  and a sloppy mapped arguments object remains linked to parameter bindings.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function duplicate(a, a) {
  return a;
}
assert.sameValue(duplicate(1, 2), 2, "the last duplicate parameter wins");

function missing(a, b, c) {
  return [a, b, c];
}
var missingResult = missing(1);
assert.sameValue(missingResult[0], 1);
assert.sameValue(missingResult[1], undefined);
assert.sameValue(missingResult[2], undefined);

function makeClosure(value) {
  return function () {
    return value;
  };
}
var firstClosure = makeClosure("first");
for (var i = 0; i < 1000; i++) {
  makeClosure(i);
}
var lastClosure = makeClosure("last");
assert.sameValue(firstClosure(), "first", "an escaped activation is not reused");
assert.sameValue(lastClosure(), "last", "later activations remain distinct");

function observeMapped(a, b) {
  a = 7;
  var first = observeMapped.arguments;
  var second = observeMapped.arguments;
  assert.sameValue(first, second, "the lazily observed object has stable identity");
  assert.sameValue(first[0], 7, "parameter writes are visible through the map");
  first[1] = 9;
  return b;
}
assert.sameValue(observeMapped(1, 2), 9, "mapped writes update the parameter");
assert.sameValue(observeMapped.arguments, null, "inactive functions expose no arguments");

function observeUnmapped(a = 1) {
  a = 8;
  return observeUnmapped.arguments[0];
}
assert.sameValue(observeUnmapped(3), 3, "non-simple parameters use an unmapped object");

function observeExtra(a) {
  $262.gc();
  return observeExtra.arguments[2].marker;
}
assert.sameValue(
  observeExtra(0, 1, { marker: 42 }),
  42,
  "deferred extra arguments remain GC-reachable"
);
