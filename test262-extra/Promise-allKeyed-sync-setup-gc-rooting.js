/*---
description: >
  Promise.allKeyed keeps its result capability reachable while enumerating and
  reading the input object's keys.
esid: sec-promise.allkeyed
info: |
  PerformPromiseAllKeyed first obtains [[OwnPropertyKeys]], then calls
  [[GetOwnProperty]] and Get for each key. Each operation can invoke user code
  and request a collection while the result capability and accumulated values
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

var fromOwnKeys = Promise.allKeyed(ownKeysInput);
var fromDescriptor = Promise.allKeyed(descriptorInput);
var fromGetter = Promise.allKeyed(getterInput);

Promise.all([fromOwnKeys, fromDescriptor, fromGetter])
  .then(function (results) {
    assert.sameValue(results[0].ownKeysValue, 1, "[[OwnPropertyKeys]] result survives");
    assert.sameValue(results[1].descriptorValue, 2, "[[GetOwnProperty]] result survives");
    assert.sameValue(results[2].getterValue, 3, "Get result survives");
  })
  .then($DONE, $DONE);
