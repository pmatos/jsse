/*---
description: >
  A throw reached while lowering an object-destructuring pattern whose
  trailing `...rest` sits beside a suspending (`await`) sibling -- either
  from a computed key's `ToPropertyKey` conversion, or from `CopyDataProperties`
  itself (e.g. a Proxy `ownKeys` trap) -- still runs `DisposeResources` for an
  active function-level `using` resource before the request promise is
  rejected, exactly like any other throw inside the async generator body.
  Regression test for the `ObjectRestCopy`/`ToPropertyKey` state-machine
  terminators added for issue #771: both terminators used to reject the
  request immediately on throw, skipping disposal.
esid: sec-asyncgeneratorstart
info: |
  AsyncGeneratorStart ( generator, generatorBody )

  4. Let result be Completion(Evaluation of generatorBody).
  [...]
  Evaluating a Block containing a `using` declaration disposes its resources
  (DisposeResources) when control leaves the block abruptly, including via a
  throw raised while evaluating a later statement in the same block.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-binding, object-rest, explicit-resource-management]
---*/

asyncTest(async function () {
  var log = [];

  var it1 = (async function* () {
    using r = { [Symbol.dispose]() { log.push('disposed'); } };
    var badKey = { toString() { throw new Error('bad key'); } };
    var { b = await 1, [badKey]: a, ...rest } = { b: undefined };
  })();
  var caught1;
  try {
    await it1.next();
  } catch (e) {
    caught1 = e;
  }
  assert.sameValue(caught1.message, 'bad key', 'the computed key conversion throws');
  assert.compareArray(
    log,
    ['disposed'],
    'the using resource is disposed before the ToPropertyKey rejection'
  );

  log = [];
  var it2 = (async function* () {
    using r = { [Symbol.dispose]() { log.push('disposed'); } };
    var p = new Proxy({}, { ownKeys() { throw new Error('ownKeys boom'); } });
    var { a = await 1, ...rest } = p;
  })();
  var caught2;
  try {
    await it2.next();
  } catch (e) {
    caught2 = e;
  }
  assert.sameValue(caught2.message, 'ownKeys boom', 'CopyDataProperties throws via the proxy trap');
  assert.compareArray(
    log,
    ['disposed'],
    'the using resource is disposed before the ObjectRestCopy rejection'
  );
});
