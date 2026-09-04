/*---
description: >
  Promise.allSettledKeyed keeps its result capability reachable while
  enumerating and reading the input object's keys.
esid: sec-promise.allsettledkeyed
info: |
  PerformPromiseAllKeyed first obtains [[OwnPropertyKeys]], then calls
  [[GetOwnProperty]] and Get for each key. Each operation can invoke user code
  and request a collection while the result capability and accumulated records
  are otherwise held only by the combinator's native implementation.
flags: [async]
features: [await-dictionary, Proxy, host-gc-required]
---*/

var ownKeysInput = new Proxy(
  { ownKeysValue: 1 },
  {
    ownKeys: function () {
      $262.gc();
      return ["ownKeysValue"];
    },
  }
);

var descriptorTarget = { descriptorValue: 2 };
var descriptorInput = new Proxy(descriptorTarget, {
  getOwnPropertyDescriptor: function (target, key) {
    $262.gc();
    return Object.getOwnPropertyDescriptor(target, key);
  },
});

var getterInput = {};
Object.defineProperty(getterInput, "getterValue", {
  configurable: true,
  enumerable: true,
  get: function () {
    $262.gc();
    return 3;
  },
});

var fromOwnKeys = Promise.allSettledKeyed(ownKeysInput);
var fromDescriptor = Promise.allSettledKeyed(descriptorInput);
var fromGetter = Promise.allSettledKeyed(getterInput);

Promise.all([fromOwnKeys, fromDescriptor, fromGetter])
  .then(function (results) {
    assert.sameValue(results[0].ownKeysValue.status, "fulfilled", "ownKeys status");
    assert.sameValue(results[0].ownKeysValue.value, 1, "[[OwnPropertyKeys]] value survives");
    assert.sameValue(results[1].descriptorValue.status, "fulfilled", "descriptor status");
    assert.sameValue(results[1].descriptorValue.value, 2, "[[GetOwnProperty]] value survives");
    assert.sameValue(results[2].getterValue.status, "fulfilled", "getter status");
    assert.sameValue(results[2].getterValue.value, 3, "Get value survives");
  })
  .then($DONE, $DONE);
