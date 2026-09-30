/*---
description: >
  In an object destructuring-assignment pattern, a computed key is evaluated
  at its own position, after the earlier properties have been read, and an
  await in it suspends the async function there rather than up front.
esid: sec-runtime-semantics-propertydestructuringassignmentevaluation
info: |
  ObjectAssignmentPattern : { AssignmentPropertyList }

  1. Perform ? RequireObjectCoercible(value).
  2. Perform ? PropertyDestructuringAssignmentEvaluation of AssignmentPropertyList
     with argument value.

  AssignmentPropertyList : AssignmentPropertyList , AssignmentProperty

  1. Perform ? PropertyDestructuringAssignmentEvaluation of AssignmentPropertyList
     with argument value.
  2. Perform ? PropertyDestructuringAssignmentEvaluation of AssignmentProperty
     with argument value.

  AssignmentProperty : PropertyName : AssignmentElement

  1. Let P be ? Evaluation of PropertyName.
  2. Perform ? KeyedDestructuringAssignmentEvaluation of AssignmentElement
     with arguments value and P.
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-assignment, computed-property-names]
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
  var a, b;
  var source = {
    get a() { L('get-a'); return 1; },
    get k() { L('get-k'); return 2; }
  };
  ({ a, [await 'k']: b } = source);
  L('b' + b);
}

async function keysInSourceOrder(L) {
  var i = 0;
  var a, b;
  ({ [i++]: a, [i++]: b = await 9 } = ['x']);
  L(a + '|' + b + '|' + i);
}

async function defaultsInSourceOrder(L) {
  var a, b;
  var source = { get a() { L('get-a'); }, get b() { L('get-b'); } };
  ({ a = await L('def-a'), b = await L('def-b') } = source);
  L('done');
}

async function rejectingKey(L) {
  var a;
  try {
    ({ [await Promise.reject(new Error('k'))]: a } = {});
    L('unreachable');
  } catch (e) {
    L('caught:' + e.message);
  }
}

Promise.all([
  run(keyAfterEarlierGetter),
  run(keysInSourceOrder),
  run(defaultsInSourceOrder),
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
  assert.compareArray(logs[3], ['sync-end', 'w1', 'caught:k', 'w2', 'w3'], 'a rejecting key is thrown at the await');
}).then($DONE, $DONE);
