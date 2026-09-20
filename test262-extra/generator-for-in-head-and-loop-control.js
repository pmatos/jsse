/*---
description: >
  Generator for-in loops evaluate the head per ForIn/OfHeadEvaluation, give
  lexical heads a fresh binding per iteration, and honour break, continue,
  return and throw across a suspension.
esid: sec-runtime-semantics-forinofheadevaluation
info: |
  ForIn/OfHeadEvaluation evaluates the RHS with the head's lexical names in
  the TDZ; an undefined or null value ends the loop without evaluating the
  body. ForIn/OfBodyEvaluation creates a new environment per lexical iteration
  and, for enumerate loops, returns an abrupt completion without calling
  IteratorClose.
includes: [compareArray.js]
features: [generators]
---*/

function* nullish(value) {
  for (var k in value) yield 'body';
  yield 'after';
}
assert.compareArray([...nullish(null)], ['after'], 'null RHS skips the body');
assert.compareArray([...nullish(undefined)], ['after'], 'undefined RHS skips the body');

function* rhsSuspends() {
  var seen = [];
  for (var k in (yield 'need object')) seen.push(k);
  return seen;
}
var it = rhsSuspends();
assert.sameValue(it.next().value, 'need object', 'RHS yield suspends first');
var res = it.next({ m: 1, n: 2 });
assert.sameValue(res.done, true, 'loop with a yield-free body finishes');
assert.compareArray(res.value, ['m', 'n'], 'resumed value is what gets enumerated');

function* lexicalRhsSuspends() {
  for (let k in (yield 1)) yield k;
}
it = lexicalRhsSuspends();
it.next();
assert.compareArray([it.next({ u: 1 }).value, it.next().value], ['u', undefined], 'let head with suspending RHS');

function* tdz() {
  for (let k in k) yield 1;
}
assert.throws(ReferenceError, function () {
  tdz().next();
}, 'lexical head name is in TDZ while the RHS is evaluated');

function* tdzCaught() {
  try {
    for (let k in k) yield 1;
  } catch (e) {
    yield e.constructor === ReferenceError;
  }
}
assert.compareArray([...tdzCaught()], [true], 'TDZ error reaches a try/catch inside the generator');

function* closures(o) {
  for (let k in o) yield function () { return k; };
  for (const [first] in o) yield function () { return first; };
}
assert.compareArray(
  [...closures({ ab: 1, cd: 2 })].map(function (f) { return f(); }),
  ['ab', 'cd', 'a', 'c'],
  'each iteration has its own binding'
);

function* nested(o, p) {
  for (let a in o) {
    for (let b in p) {
      yield function () { return a + b; };
    }
  }
}
assert.compareArray(
  [...nested({ x: 1, y: 1 }, { 1: 1, 2: 1 })].map(function (f) { return f(); }),
  ['x1', 'x2', 'y1', 'y2'],
  'nested loops keep every active iteration environment across yield'
);

function* insideForOf() {
  for (var v of [10, 20]) {
    for (var k in { a: 1, b: 2 }) yield v + k;
  }
}
assert.compareArray([...insideForOf()], ['10a', '10b', '20a', '20b'], 'for-in inside for-of');

function* breaks(o) {
  for (var k in o) {
    if (k === 'b') break;
    yield k;
  }
  yield 'done';
}
assert.compareArray([...breaks({ a: 1, b: 2, c: 3 })], ['a', 'done'], 'break');

function* continues(o) {
  for (var k in o) {
    if (k === 'b') continue;
    yield k;
  }
}
assert.compareArray([...continues({ a: 1, b: 2, c: 3 })], ['a', 'c'], 'continue');

function* labelled(o) {
  outer: for (var i of [1, 2]) {
    for (var k in o) {
      if (k === 'b') continue outer;
      yield i + k;
    }
  }
  loop: for (var k in o) {
    for (var j of [1, 2]) {
      if (k === 'b') break loop;
      yield k + j;
    }
  }
}
assert.compareArray(
  [...labelled({ a: 1, b: 2 })],
  ['1a', '2a', 'a1', 'a2'],
  'labelled continue and break across for-in/for-of'
);

function* returns(o) {
  for (var k in o) {
    yield k;
    return 'ret';
  }
}
it = returns({ a: 1, b: 2 });
assert.sameValue(it.next().value, 'a', 'first key');
res = it.next();
assert.sameValue(res.value, 'ret', 'return value');
assert.sameValue(res.done, true, 'return completes the generator');

function* throwsInBody(o) {
  try {
    for (var k in o) {
      yield k;
      throw new Test262Error('boom');
    }
  } catch (e) {
    yield e.message;
  }
}
assert.compareArray([...throwsInBody({ a: 1 })], ['a', 'boom'], 'throw in body caught by an outer try');

function* finallyInBody(o, log) {
  for (var k in o) {
    try {
      yield k;
    } finally {
      log.push('f' + k);
    }
  }
}
var log = [];
assert.compareArray([...finallyInBody({ a: 1, b: 2 }, log)], ['a', 'b'], 'try/finally in the body');
assert.compareArray(log, ['fa', 'fb'], 'finalizers ran once per iteration');

log = [];
it = finallyInBody({ a: 1, b: 2 }, log);
it.next();
res = it.return('early');
assert.sameValue(res.value, 'early', 'return() value');
assert.sameValue(res.done, true, 'return() completes the generator');
assert.compareArray(log, ['fa'], 'return() runs the pending finalizer');
assert.sameValue(it.next().done, true, 'generator stays completed');

log = [];
it = finallyInBody({ a: 1, b: 2 }, log);
it.next();
assert.throws(Test262Error, function () {
  it.throw(new Test262Error('injected'));
}, 'throw() propagates out');
assert.compareArray(log, ['fa'], 'throw() runs the pending finalizer');
assert.sameValue(it.next().done, true, 'generator stays completed after throw()');
