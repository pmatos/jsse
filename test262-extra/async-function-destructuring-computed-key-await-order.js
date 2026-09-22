/*---
description: >
  A computed key in an object binding pattern is evaluated at its own
  position, after the earlier properties have been read, and an await in it
  suspends the async function there rather than up front.
esid: sec-destructuring-binding-patterns-runtime-semantics-propertybindinginitialization
info: |
  BindingPropertyList : BindingPropertyList , BindingProperty

  1. Perform ? PropertyBindingInitialization of BindingPropertyList ...
  2. Perform ? PropertyBindingInitialization of BindingProperty ...

  BindingProperty : PropertyName : BindingElement

  1. Let P be ? Evaluation of PropertyName.
  2. Perform ? KeyedBindingInitialization of BindingElement ... with P.
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding, computed-property-names]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  var ticks = Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return Promise.all([p, ticks]).then(function () { return log; });
}

async function keyAfterEarlierGetter(L) {
  var source = {
    get a() { L('get-a'); return 1; },
    get k() { L('get-k'); return 2; }
  };
  var { a, [await 'k']: b } = source;
  L('b' + b);
}

async function keysInSourceOrder(L) {
  var i = 0;
  var { [i++]: a, [i++]: b = await 9 } = ['x'];
  L(a + '|' + b + '|' + i);
}

async function defaultsInSourceOrder(L) {
  var source = { get a() { L('get-a'); }, get b() { L('get-b'); } };
  var { a = await L('def-a'), b = await L('def-b') } = source;
  L('done');
}

async function numericAndStringKeys(L) {
  var { 1: a = await 1, 'x y': b = await 2, [1 + 1]: c = await 3 } = { 2: 'two' };
  L('' + a + b + c);
}

async function rejectingKey(L) {
  try {
    var { [await Promise.reject(new Error('k'))]: a } = {};
    L('unreachable');
  } catch (e) {
    L('caught:' + e.message);
  }
}

Promise.all([
  run(keyAfterEarlierGetter),
  run(keysInSourceOrder),
  run(defaultsInSourceOrder),
  run(numericAndStringKeys),
  run(rejectingKey),
]).then(function (logs) {
  assert.compareArray(
    logs[0],
    ['get-a', 'sync-end', 'w1', 'get-k', 'b2', 'w2', 'w3'],
    'the getter of a runs before the key await; the getter of k runs after it'
  );
  assert.compareArray(logs[1], ['sync-end', 'w1', 'x|9|2', 'w2', 'w3'], 'computed keys evaluated in order, once');
  assert.compareArray(
    logs[2],
    ['get-a', 'def-a', 'sync-end', 'w1', 'get-b', 'def-b', 'w2', 'done', 'w3'],
    'each property is read just before its own default'
  );
  assert.compareArray(logs[3], ['sync-end', 'w1', 'w2', '12two', 'w3'], 'literal and computed keys');
  assert.compareArray(logs[4], ['sync-end', 'w1', 'caught:k', 'w2', 'w3'], 'a rejecting key is thrown at the await');
}).then($DONE, $DONE);
