/*---
description: >
  An async generator that runs, yields from, or returns a different async
  generator inside its own for-await body neither closes nor re-enters the
  outer async generator's for-await iterators.
esid: sec-asyncgeneratorstart
info: |
  Each async generator resumes its own execution context. Nested
  AsyncGenerator.prototype.next/return calls made from the body of a for-await
  loop only affect the receiver's own suspended evaluation.

  ForIn/OfBodyEvaluation (iteratorKind async): the loop's iterator is closed
  with AsyncIteratorClose only when the loop's own generator is returned or
  thrown into, never by an unrelated async generator.
includes: [compareArray.js]
flags: [async]
features: [async-iteration]
---*/

async function* range(a, b) {
  for (var i = a; i <= b; i++) yield i;
}

var wrapperReturns = 0;
async function* outer() {
  var g = range(1, 4);
  var wrapper = {
    [Symbol.asyncIterator]() { return this; },
    next(v) { return g.next(v); },
    return(v) { wrapperReturns++; return g.return(v); },
  };
  for await (var d of wrapper) {
    var h = range(1, 3);
    await h.next();
    var r = await h.return();
    assert.sameValue(r.value, undefined, 'nested return() value');
    assert.sameValue(r.done, true, 'nested return() done');
    yield d;
  }
}

async function collect(it) {
  var out = [];
  for await (var v of it) out.push(v);
  return out;
}

function rangeIterable(a, b) {
  return { async *[Symbol.asyncIterator]() { for (var i = a; i <= b; i++) yield i; } };
}
function map(src, f) {
  return { async *[Symbol.asyncIterator]() { for await (var d of src) yield f(d); } };
}
function chunk(src, n) {
  return {
    async *[Symbol.asyncIterator]() {
      var c = [];
      for await (var d of src) {
        c.push(d);
        if (c.length === n) { yield c; c = []; }
      }
    }
  };
}
function take(src, n) {
  return {
    async *[Symbol.asyncIterator]() {
      if (n <= 0) return;
      for await (var d of src) {
        yield d;
        if (--n <= 0) return;
      }
    }
  };
}

var log = [];
function trackedIterable() {
  var i = 0;
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: ++i, done: false }); },
    return() { log.push('close:tracked'); return Promise.resolve({}); },
  };
}
async function* outerClosesOwn() {
  for await (var x of trackedIterable()) {
    var h = range(1, 2);
    await h.next();
    await h.next();
    await h.next();
    yield x;
  }
}

var nestedLog = [];
function trackedIterableNamed(name) {
  var i = 0;
  return {
    [Symbol.asyncIterator]() { return this; },
    next() { return Promise.resolve({ value: ++i, done: false }); },
    return() { nestedLog.push('close:' + name); return Promise.resolve({}); },
  };
}
async function* suspendedInLoop() {
  for await (var y of trackedIterableNamed('B')) yield y;
}
async function* callerReturnsNested() {
  for await (var x of trackedIterableNamed('A')) {
    var b = suspendedInLoop();
    await b.next();
    await b.return();
    yield x;
  }
}

(async function () {
  assert.compareArray(await collect(outer()), [1, 2, 3, 4], 'outer async generator keeps producing');
  assert.sameValue(wrapperReturns, 0, 'nested return() did not close the outer iterator');

  var pipeline = take(chunk(map(rangeIterable(1, Infinity), function (x) { return x * 2; }), 2), 4);
  assert.sameValue(
    JSON.stringify(await collect(pipeline)),
    '[[2,4],[6,8],[10,12],[14,16]]',
    'nested async generator pipeline with early return'
  );

  var oc = outerClosesOwn();
  assert.sameValue((await oc.next()).value, 1, 'first value');
  assert.sameValue((await oc.next()).value, 2, 'second value');
  assert.compareArray(log, [], 'not closed while suspended');
  await oc.return();
  assert.compareArray(log, ['close:tracked'], 'outer return() closes its own iterator exactly once');

  var cr = callerReturnsNested();
  assert.sameValue((await cr.next()).value, 1, 'caller first value');
  assert.sameValue((await cr.next()).value, 2, 'caller second value');
  assert.compareArray(nestedLog, ['close:B', 'close:B'], 'only the nested generator iterator was closed');
  await cr.return();
  assert.compareArray(
    nestedLog,
    ['close:B', 'close:B', 'close:A'],
    'caller return() closes its own iterator exactly once'
  );
})().then($DONE, $DONE);
