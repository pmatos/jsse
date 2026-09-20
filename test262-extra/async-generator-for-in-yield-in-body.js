/*---
description: >
  An async generator for-in loop whose body yields or awaits still enumerates
  the object, with per-iteration bindings and correct loop control.
esid: sec-runtime-semantics-forinofloopevaluation
info: |
  ForIn/OfBodyEvaluation steps the enumerator, binds each key and evaluates the
  body. AsyncGeneratorYield and Await suspend the generator without losing the
  enumerator or the active iteration environment.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

async function collect(gen) {
  var out = [];
  for await (var v of gen) out.push(v);
  return out;
}

async function* issueShape(o) {
  for (var k in o) yield k;
  yield 'after';
}

async function* awaiting(o) {
  for (var k in o) {
    yield await Promise.resolve(k + '!');
  }
}

async function* closures(o) {
  for (let k in o) yield function () { return k; };
}

async function* control(o) {
  outer: for (var i of [1, 2]) {
    for (var k in o) {
      if (k === 'b') continue outer;
      if (i === 2 && k === 'a') break outer;
      yield i + k;
    }
  }
  yield 'end';
}

async function* nullish(o) {
  for (var k in o) yield 'body';
  yield 'after';
}

async function* rhsAwaits() {
  for (var k in await Promise.resolve({ r: 1, s: 2 })) yield k;
}

async function* withFinally(o, log) {
  for (var k in o) {
    try {
      yield k;
    } finally {
      log.push('f' + k);
    }
  }
}

asyncTest(async function () {
  assert.compareArray(await collect(issueShape({ a: 1, b: 2 })), ['a', 'b', 'after'], 'yield in body');
  assert.compareArray(await collect(awaiting({ a: 1, b: 2 })), ['a!', 'b!'], 'await and yield in body');

  var fns = await collect(closures({ ab: 1, cd: 2 }));
  assert.compareArray(fns.map(function (f) { return f(); }), ['ab', 'cd'], 'per-iteration bindings');

  assert.compareArray(await collect(control({ a: 1, b: 2 })), ['1a', 'end'], 'labelled continue and break');
  assert.compareArray(await collect(nullish(null)), ['after'], 'null RHS skips the body');
  assert.compareArray(await collect(rhsAwaits()), ['r', 's'], 'await in the RHS');

  var log = [];
  var it = withFinally({ a: 1, b: 2 }, log);
  assert.sameValue((await it.next()).value, 'a', 'first key');
  var res = await it.return('early');
  assert.sameValue(res.value, 'early', 'return() value');
  assert.sameValue(res.done, true, 'return() completes the generator');
  assert.compareArray(log, ['fa'], 'return() runs the pending finalizer');
});
