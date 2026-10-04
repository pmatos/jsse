/*---
description: >
  An await in a nested binding pattern's default suspends the async function,
  and abrupt completions (a rejecting default, a null nested source) route to
  the enclosing try statement at the right time.
esid: sec-runtime-semantics-bindinginitialization
info: |
  BindingPattern : ObjectBindingPattern

  1. Perform ? RequireObjectCoercible(value).
  2. Return ? BindingInitialization of ObjectBindingPattern with argument value and environment.
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  var ticks = Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return Promise.all([p, ticks]).then(function () { return log; });
}

async function nested(L) { var { x: { a = await 1 } } = { x: {} }; L('a' + a); }
async function nestedWithOwnDefault(L) { var { x: { a = await 1 } = {} } = {}; L('a' + a); }
async function nestedInitializerAwait(L) {
  var { x: { a } = await Promise.resolve({ a: 3 }) } = {};
  L('a' + a);
}
async function rejectingDefaultVar(L) {
  try {
    var { a = await Promise.reject(new Error('boom')) } = {};
    L('unreachable');
  } catch (e) {
    L('caught:' + e.message);
  }
  L('a:' + typeof a);
}
async function rejectingDefaultLet(L) {
  try {
    let { a = await Promise.reject(new Error('boom')) } = {};
    L('unreachable');
  } catch (e) {
    L('caught:' + e.message);
  }
}
async function nullNestedSource(L) {
  try {
    var { x: { a = await 1 } } = { x: null };
    L('unreachable');
  } catch (e) {
    L(e.constructor.name);
  }
}
async function anonymousFunctionNames(L) {
  var { f = function () {}, g = await 1, h = () => {} } = {};
  L(f.name + g + h.name);
}
async function insideTryFinally(L) {
  try {
    var { a = await 1 } = {};
    L('a' + a);
  } finally {
    L('fin');
  }
}
async function loopClosures(L) {
  var fns = [];
  for (let i = 0; i < 2; i++) {
    let { a = await i } = {};
    fns.push(function () { return a + ':' + i; });
  }
  L(fns.map(function (f) { return f(); }).join('|'));
}
async function constClosure(L) {
  const { a = await 1 } = {};
  var f = function () { return a; };
  L('a' + f());
}

Promise.all([
  run(nested),
  run(nestedWithOwnDefault),
  run(nestedInitializerAwait),
  run(rejectingDefaultVar),
  run(rejectingDefaultLet),
  run(nullNestedSource),
  run(anonymousFunctionNames),
  run(insideTryFinally),
  run(loopClosures),
  run(constClosure),
]).then(function (logs) {
  assert.compareArray(logs[0], ['sync-end', 'w1', 'a1', 'w2', 'w3'], 'nested');
  assert.compareArray(logs[1], ['sync-end', 'w1', 'a1', 'w2', 'w3'], 'nested with its own default');
  assert.compareArray(logs[2], ['sync-end', 'w1', 'a3', 'w2', 'w3'], 'await in a nested default value');
  assert.compareArray(logs[3], ['sync-end', 'w1', 'caught:boom', 'a:undefined', 'w2', 'w3'], 'rejecting default, var');
  assert.compareArray(logs[4], ['sync-end', 'w1', 'caught:boom', 'w2', 'w3'], 'rejecting default, let');
  assert.compareArray(logs[5], ['TypeError', 'sync-end', 'w1', 'w2', 'w3'], 'null nested source throws before the await');
  assert.compareArray(logs[6], ['sync-end', 'w1', 'f1h', 'w2', 'w3'], 'anonymous functions keep their binding names');
  assert.compareArray(logs[7], ['sync-end', 'w1', 'a1', 'fin', 'w2', 'w3'], 'try/finally');
  assert.compareArray(logs[8], ['sync-end', 'w1', 'w2', '0:0|1:1', 'w3'], 'per-iteration let bindings');
  assert.compareArray(logs[9], ['sync-end', 'w1', 'a1', 'w2', 'w3'], 'const binding is readable after the await');
}).then($DONE, $DONE);
