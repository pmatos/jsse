/*---
description: >
  A generator that starts an async function which is suspended inside a for-of
  loop does not close that loop's iterator when the generator is returned.
esid: sec-generator.prototype.return
info: |
  GeneratorResumeAbrupt resumes only the receiver's execution context, so only
  the iterators held by the receiver's own for-of loops are closed by return().
  A loop suspended inside an async function belongs to that function's
  execution context, and it is closed only when that loop exits abruptly.
includes: [compareArray.js]
flags: [async]
features: [generators, async-functions]
---*/

var log = [];
function finite(name, count) {
  var i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { return { value: ++i, done: i > count }; },
    return() { log.push('close:' + name); return {}; },
  };
}

async function loopWithAwait(iterable) {
  for (var x of iterable) {
    await null;
  }
  log.push('loop-done');
}

var pending;
function* g() {
  pending = loopWithAwait(finite('async-fn', 3));
  yield 1;
  yield 2;
}

var it = g();
it.next();
it.return();
assert.compareArray(log, [], 'return() did not close the async function iterator');

pending.then(function () {
  assert.compareArray(log, ['loop-done'], 'async function loop ran to completion without being closed');
}).then($DONE, $DONE);
