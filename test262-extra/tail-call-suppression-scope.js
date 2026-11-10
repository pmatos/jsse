/*---
description: Tail-call suppression retains pending handlers and does not leak into child calls or resumed jobs.
esid: sec-static-semantics-hascallintailposition
flags: [onlyStrict, async]
features: [tail-call-optimization, async-functions, async-iteration]
includes: [compareArray.js]
---*/

var log = [];
var reason = {};
function fail() { log.push('call'); throw reason; }
function leaf() { log.push('leaf'); return 23; }
function guarded() {
  try { return fail(); }
  catch (error) { assert.sameValue(error, reason); log.push('catch'); return 17; }
}
assert.sameValue(guarded(), 17);
assert.compareArray(log, ['call', 'catch']);

log = [];
function catchWithFinally() {
  try { throw reason; }
  catch (error) { assert.sameValue(error, reason); return leaf(); }
  finally { log.push('finally'); }
}
assert.sameValue(catchWithFinally(), 23);
assert.compareArray(log, ['leaf', 'finally']);

log = [];
function nestedFinalizer() {
  try {
    try { return 0; }
    finally { return fail(); }
  } catch (error) { assert.sameValue(error, reason); log.push('outer'); return 29; }
}
assert.sameValue(nestedFinalizer(), 29);
assert.compareArray(log, ['call', 'outer']);

log = [];
var iterable = {
  [Symbol.iterator]() {
    return {
      next() { return { value: 1, done: false }; },
      return() { log.push('close'); return {}; }
    };
  }
};
function fromLoop() {
  for (var value of iterable) { return leaf(); }
}
assert.sameValue(fromLoop(), 23);
assert.compareArray(log, ['leaf', 'close']);

// A called function gets its own tail positions, even under a live handler.
function recur(n) { return n === 0 ? 31 : recur(n - 1); }
function parent() {
  try { return recur(2000); }
  finally { log.push('parent-finally'); }
}
assert.sameValue(parent(), 31);

function* fromGenerator() {
  yield 0;
  return recur(2000);
}
var syncGenerator = fromGenerator();
assert.sameValue(syncGenerator.next().value, 0);
var syncLast = syncGenerator.next();
assert.sameValue(syncLast.value, 31);
assert.sameValue(syncLast.done, true);

(async function () {
  log = [];
  async function fromAsync() {
    await 0;
    assert.sameValue(recur(2000), 31);
    try { return true ? fail() : 0; }
    catch (error) { assert.sameValue(error, reason); log.push('async-catch'); return 37; }
    finally { log.push('async-finally'); }
  }
  assert.sameValue(await fromAsync(), 37);
  assert.compareArray(log, ['call', 'async-catch', 'async-finally']);

  log = [];
  async function* fromAsyncGenerator() {
    await 0;
    yield 41;
    assert.sameValue(recur(2000), 31);
    try { return true ? fail() : 0; }
    catch (error) { assert.sameValue(error, reason); log.push('gen-catch'); return 43; }
    finally { log.push('gen-finally'); }
  }
  var generator = fromAsyncGenerator();
  var first = await generator.next();
  assert.sameValue(first.value, 41);
  assert.sameValue(first.done, false);
  var last = await generator.next();
  assert.sameValue(last.value, 43);
  assert.sameValue(last.done, true);
  assert.compareArray(log, ['call', 'gen-catch', 'gen-finally']);
  assert.sameValue(recur(2000), 31);
})().then(function () { $DONE(); }, $DONE);
