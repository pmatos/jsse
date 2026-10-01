/*---
description: >
  An object rest element following a property whose default suspends on
  `await` is still lowered into the state-machine transform: the async
  function actually suspends at the await (rather than blocking
  synchronously on the tree-walker's replay fallback), and the rest object
  excludes every key already consumed by an earlier property, in source
  order (issue #771).
esid: sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization
info: |
  ObjectBindingPattern : { BindingPropertyList , BindingRestProperty }

  1. Let excludedNames be ? PropertyBindingInitialization of
     BindingPropertyList ...
  2. Perform ? RestBindingInitialization of BindingRestProperty with
     excludedNames.

  RestBindingInitialization

  BindingRestProperty : ... BindingIdentifier

  1. Let lhs be ? ResolveBinding(...)
  2. Let restObj be OrdinaryObjectCreate(%Object.prototype%).
  3. Perform ? CopyDataProperties(restObj, value, excludedNames).
  4. Return ? InitializeReferencedBinding(lhs, restObj).
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding, object-rest]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  var ticks = Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); });
  var p = makeFn(L);
  L('sync-end');
  return Promise.all([p, ticks]).then(function () { return log; });
}

async function suspendsAtTheAwait(L) {
  L('before');
  var { a = await 1, ...rest } = { b: 2, c: 3 };
  L('after:' + a + ':' + JSON.stringify(rest));
}

async function excludesPrecedingKeys() {
  var { a, b = 2, ...rest } = { a: 1, c: 3, d: 4 };
  return { a: a, b: b, rest: rest };
}

async function coercesPrimitiveSource() {
  var { a = await 1, ...rest } = 'xy';
  return { a: a, rest: rest };
}

Promise.all([
  run(suspendsAtTheAwait),
  excludesPrecedingKeys(),
  coercesPrimitiveSource(),
]).then(function (results) {
  assert.compareArray(
    results[0],
    ['before', 'sync-end', 'w1', 'after:1:{"b":2,"c":3}', 'w2'],
    'the function actually suspends at the await — synchronous code after ' +
      'calling it, and a microtask queued before it, both run first'
  );

  var r1 = results[1];
  assert.sameValue(r1.a, 1, 'a preceding non-defaulted property is bound');
  assert.sameValue(r1.b, 2, 'the default runs when the property is absent');
  assert.sameValue(Object.keys(r1.rest).length, 2, 'rest excludes a and b only');
  assert.sameValue(r1.rest.c, 3, 'rest keeps properties not named by the pattern');
  assert.sameValue(r1.rest.d, 4, 'rest keeps properties not named by the pattern');

  var r2 = results[2];
  assert.sameValue(r2.a, 1, 'a primitive source is coerced via ToObject before the rest copy');
  assert.sameValue(r2.rest[0], 'x', 'a string source exposes its own character-index properties');
  assert.sameValue(r2.rest[1], 'y', 'a string source exposes its own character-index properties');
}).then($DONE, $DONE);
