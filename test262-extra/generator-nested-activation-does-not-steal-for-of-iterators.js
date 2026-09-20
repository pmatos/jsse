/*---
description: >
  A generator that runs, yields from, or returns a different generator inside
  its own for-of body neither closes nor re-enters the outer generator's
  for-of iterators.
esid: sec-generatorvalidate
info: |
  GeneratorValidate throws a TypeError only when the receiver's own
  [[GeneratorState]] is executing. GeneratorResumeAbrupt (Generator.prototype.return)
  resumes only the receiver's execution context, so only the iterators that
  the receiver's own for-of loops hold are closed.

  ForIn/OfBodyEvaluation: a yield inside the loop body merely suspends the
  generator; the loop's iterator is closed when that suspended generator is
  returned or thrown into, and never by an unrelated generator.
includes: [compareArray.js]
features: [generators]
---*/

function* range(a, b) {
  for (var i = a; i <= b; i++) yield i;
}

// A nested generator's return() must not call return() on the outer for-of
// iterator.
var wrapperReturns = 0;
function* outer() {
  var g = range(1, 4);
  var wrapper = {
    [Symbol.iterator]() { return this; },
    next(v) { return g.next(v); },
    return(v) { wrapperReturns++; return g.return(v); },
  };
  for (var d of wrapper) {
    var h = range(1, 3);
    h.next();
    var r = h.return();
    assert.sameValue(r.value, undefined, 'nested return() value');
    assert.sameValue(r.done, true, 'nested return() done');
    yield d;
  }
}

assert.compareArray([...outer()], [1, 2, 3, 4], 'outer generator keeps producing');
assert.sameValue(wrapperReturns, 0, 'nested return() did not close the outer iterator');

// A nested generator that is only advanced to a yield must not take the
// outer generator's open iterators with it.
var advancedReturns = 0;
function* outerAdvancing() {
  var g = range(1, 3);
  var wrapper = {
    [Symbol.iterator]() { return this; },
    next(v) { return g.next(v); },
    return(v) { advancedReturns++; return g.return(v); },
  };
  for (var d of wrapper) {
    var h = range(10, 12);
    h.next();
    yield d;
    h.return();
  }
}

assert.compareArray([...outerAdvancing()], [1, 2, 3], 'outer generator with advancing nested generator');
assert.sameValue(advancedReturns, 0, 'advancing a nested generator did not close the outer iterator');

// The lazy-collections shape: a pipeline of generator-backed iterables where
// the consumer returns early out of a for-of over a nested generator.
function rangeIterable(a, b) {
  return { *[Symbol.iterator]() { for (var i = a; i <= b; i++) yield i; } };
}
function map(src, f) {
  return { *[Symbol.iterator]() { for (var d of src) yield f(d); } };
}
function chunk(src, n) {
  return {
    *[Symbol.iterator]() {
      var c = [];
      for (var d of src) {
        c.push(d);
        if (c.length === n) { yield c; c = []; }
      }
    }
  };
}
function take(src, n) {
  return {
    *[Symbol.iterator]() {
      if (n <= 0) return;
      for (var d of src) {
        yield d;
        if (--n <= 0) return;
      }
    }
  };
}

var pipeline = take(chunk(map(rangeIterable(1, Infinity), function (x) { return x * 2; }), 2), 4);
assert.sameValue(
  JSON.stringify([...pipeline]),
  '[[2,4],[6,8],[10,12],[14,16]]',
  'nested generator pipeline with early return'
);

function* naturals() { for (var i = 1; ; i++) yield i; }
function* filterIterable(src, p) { for (var d of src) if (p(d)) yield d; }
function isPrime(n) {
  if (n < 2) return false;
  for (var i = 2; i * i <= n; i++) if (n % i === 0) return false;
  return true;
}
function firstN(src, n) {
  var out = [];
  if (n <= 0) return out;
  for (var d of src) {
    out.push(d);
    if (out.length >= n) break;
  }
  return out;
}
assert.compareArray(
  firstN(filterIterable(naturals(), isPrime), 3),
  [2, 3, 5],
  'early break out of a for-of over a generator that itself iterates a generator'
);

var pr = { *[Symbol.iterator]() { yield* filterIterable(naturals(), isPrime); } };
function* takeFromInner(src, n) {
  for (var d of src) {
    yield firstN(filterIterable(naturals(), isPrime), 2)[1] + d;
    if (--n <= 0) return;
  }
}
assert.compareArray([...takeFromInner(pr, 3)], [3 + 2, 3 + 3, 3 + 5], 'nested generators driven from an outer for-of body');

// The outer generator's own return() still closes its own open for-of
// iterator exactly once after a nested activation ran between yields.
var log = [];
function trackedIterable() {
  var i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { return { value: ++i, done: false }; },
    return() { log.push('close:tracked'); return {}; },
  };
}
function* outerClosesOwn() {
  for (var x of trackedIterable()) {
    var h = range(1, 2);
    h.next();
    h.next();
    h.next();
    yield x;
  }
}
var oc = outerClosesOwn();
assert.sameValue(oc.next().value, 1, 'first value');
assert.sameValue(oc.next().value, 2, 'second value');
assert.compareArray(log, [], 'not closed while suspended');
oc.return();
assert.compareArray(log, ['close:tracked'], 'outer return() closes its own iterator exactly once');

// return() on a nested generator that is itself suspended inside a for-of
// closes only that generator's iterator, and leaves the caller's open
// for-of iterator alone.
var nestedLog = [];
function trackedIterableNamed(name) {
  var i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { return { value: ++i, done: false }; },
    return() { nestedLog.push('close:' + name); return {}; },
  };
}
function* suspendedInLoop() {
  for (var y of trackedIterableNamed('B')) yield y;
}
function* callerReturnsNested() {
  for (var x of trackedIterableNamed('A')) {
    var b = suspendedInLoop();
    b.next();
    b.return();
    yield x;
  }
}
var cr = callerReturnsNested();
assert.sameValue(cr.next().value, 1, 'caller first value');
assert.sameValue(cr.next().value, 2, 'caller second value');
assert.compareArray(nestedLog, ['close:B', 'close:B'], 'only the nested generator iterator was closed');
cr.return();
assert.compareArray(
  nestedLog,
  ['close:B', 'close:B', 'close:A'],
  'caller return() closes its own iterator exactly once'
);
