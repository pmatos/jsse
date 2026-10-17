/*---
description: >
  next, return and throw on a generator at its start, suspended at a yield,
  running and completed (sec-generatorresume, sec-generatorresumeabrupt);
  the first next() argument is discarded and later ones are the value of the
  yield they resume.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
features: [generators]
---*/

function show(r) {
  return (r.done ? "R:" : "Y:") + String(r.value);
}

// next(v): the first argument is dropped, the rest become the yield's value.
function* echo() {
  const a = yield "first";
  const b = yield "a=" + a;
  return "b=" + b;
}
var it = echo();
assert.sameValue(show(it.next("dropped")), "Y:first");
assert.sameValue(show(it.next("A")), "Y:a=A");
assert.sameValue(show(it.next("B")), "R:b=B");
assert.sameValue(show(it.next("C")), "R:undefined", "next after completion");

// return() before the first next() completes without running the body.
var ran = false;
function* neverStarted() { ran = true; yield 1; }
it = neverStarted();
assert.sameValue(show(it.return("early")), "R:early");
assert.sameValue(ran, false, "body not started");
assert.sameValue(show(it.next()), "R:undefined");
assert.sameValue(show(it.return("again")), "R:again", "return after completion");

// throw() before the first next() throws at the caller and completes it.
it = neverStarted();
assert.throws(Test262Error, function() { it.throw(new Test262Error()); });
assert.sameValue(ran, false);
assert.sameValue(show(it.next()), "R:undefined");

// throw() after completion rethrows.
it = echo();
it.next(); it.next(); it.next();
assert.throws(Test262Error, function() { it.throw(new Test262Error()); });

// An uncaught throw completes the generator.
function* thrower() { yield 1; throw new Test262Error(); }
it = thrower();
it.next();
assert.throws(Test262Error, function() { it.next(); });
assert.sameValue(show(it.next()), "R:undefined");

// A running generator cannot be resumed from inside.
var self;
function* reentrant() {
  var results = [];
  for (const method of ["next", "return", "throw"]) {
    try { self[method](1); results.push("no error"); }
    catch (e) { results.push(e.constructor.name); }
  }
  yield results.join(",");
}
self = reentrant();
assert.sameValue(show(self.next()), "Y:TypeError,TypeError,TypeError");

// Generator function parameters, this and arguments reach the body.
function* params(a, b = a + 1, ...rest) {
  yield [a, b, rest.length, arguments.length].join(",");
}
assert.sameValue(show(params.call(undefined, 1, undefined, 3, 4).next()), "Y:1,2,2,4");
function* strictThis() { "use strict"; yield this; }
assert.sameValue(strictThis.call(undefined).next().value, undefined);
assert.sameValue(strictThis.call(7).next().value, 7);

// Results are fresh objects with the standard shape.
function* shape() { yield 1; }
it = shape();
var r1 = it.next();
assert.sameValue(Object.getPrototypeOf(r1), Object.prototype);
assert.sameValue(Object.keys(r1).join(), "value,done");
assert.notSameValue(r1, it.next());

// A generator closes over its arguments and variables independently.
function* counter(start) { var n = start; while (true) { n += yield n; } }
var c1 = counter(10), c2 = counter(100);
c1.next(); c2.next();
assert.sameValue(c1.next(1).value, 11);
assert.sameValue(c2.next(5).value, 105);
assert.sameValue(c1.next(1).value, 12);
