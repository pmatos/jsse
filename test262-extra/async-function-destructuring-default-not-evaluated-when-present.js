/*---
description: >
  A destructuring default is evaluated only when the property value is
  undefined, and the source property is read exactly once. A present
  property therefore neither runs the default's await nor costs a tick.
esid: sec-runtime-semantics-keyedbindinginitialization
info: |
  KeyedBindingInitialization : BindingElement : BindingPattern Initializer_opt

  1. Let v be ? GetV(value, propertyName).
  2. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
    b. Set v to ? GetValue(defaultValue).
  ...
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); });
  var p = makeFn(L);
  L('sync-end');
  return p.then(function () { return log; });
}

async function present(L) {
  var evaluated = 0;
  var { a = await (evaluated++, 1) } = { a: 5 };
  L('a' + a + ' evaluated' + evaluated);
}

async function presentThenAwait(L) {
  var { a = await 1 } = { a: 5 };
  L('a' + a);
  await 0;
  L('after');
}

async function nullValueIsPresent(L) {
  var { a = await 1 } = { a: null };
  L('a' + a);
}

async function getterFiresOnce(L) {
  var reads = 0;
  var source = { get a() { reads++; L('get-a'); return undefined; } };
  var { a = await 7 } = source;
  L('reads' + reads + ' a' + a);
}

async function undefinedShadowed(L) {
  var undefined = 5;
  var { a = await 1 } = { a: undefined };
  L('a' + a);
}

async function primitiveSourcesWithPresentProperties(L) {
  var { length = await 1 } = 'abc';
  var { toFixed = await 2 } = 5;
  L('len' + length + ' ' + typeof toFixed);
}

async function nullSource(L) {
  try {
    var { a = await 1 } = null;
    L('unreachable');
  } catch (e) {
    L(e.constructor.name);
  }
}

Promise.all([
  run(present),
  run(presentThenAwait),
  run(nullValueIsPresent),
  run(getterFiresOnce),
  run(undefinedShadowed),
  run(primitiveSourcesWithPresentProperties),
  run(nullSource),
]).then(function (logs) {
  assert.compareArray(logs[0], ['a5 evaluated0', 'sync-end', 'w1', 'w2'], 'default not evaluated, no tick');
  assert.compareArray(logs[1], ['a5', 'sync-end', 'w1', 'after', 'w2'], 'only the later await suspends');
  assert.compareArray(logs[2], ['anull', 'sync-end', 'w1', 'w2'], 'null is not undefined');
  assert.compareArray(logs[3], ['get-a', 'sync-end', 'w1', 'reads1 a7', 'w2'], 'source getter fires once');
  assert.compareArray(logs[4], ['a5', 'sync-end', 'w1', 'w2'], 'the identifier undefined can be shadowed');
  assert.compareArray(logs[5], ['len3 function', 'sync-end', 'w1', 'w2'], 'present primitive properties');
  assert.compareArray(logs[6], ['TypeError', 'sync-end', 'w1', 'w2'], 'null source throws before any await');
}).then($DONE, $DONE);
