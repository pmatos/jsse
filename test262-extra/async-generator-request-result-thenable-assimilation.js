// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorcompletestep
description: >
  The promise returned by next/return/throw is resolved with the iterator
  result object through its resolve function, so a `then` found on
  Object.prototype is looked up (once per settled request) and a callable one
  assimilates the result, one extra job later.
info: |
  AsyncGeneratorCompleteStep ( generator, completion, done [ , realm ] )

  [...]
  8. Else,
     a. Let iteratorResult be CreateIterResultObject(value, done).
  9. Perform ! Call(promiseCapability.[[Resolve]], undefined, « iteratorResult »).

  Promise Resolve Functions

  [...]
  9. Let then be Completion(Get(resolution, "then")).
  [...]
  12. Let thenJobCallback be HostMakeJobCallback(thenAction).
  13. Let job be NewPromiseResolveThenableJob(promise, resolution, thenJobCallback).
  14. Perform HostEnqueuePromiseJob(job.[[Job]], job.[[Realm]]).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

var log = [];
var seen;

async function* yielder() {
  yield 'a';
  yield 'b';
  return 'c';
}

async function* thrower() {
  throw new Error('boom');
}

asyncTest(async function () {
  Object.defineProperty(Object.prototype, 'then', {
    configurable: true,
    get: function () {
      log.push('get then ' + (this.done === undefined ? 'other' : this.done));
      return undefined;
    }
  });
  try {
    var it = yielder();
    var first = await it.next();
    var second = await it.next();
    var third = await it.return('r');
    assert.sameValue(first.value, 'a');
    assert.sameValue(second.value, 'b');
    assert.sameValue(third.value, 'r');
    assert.sameValue(third.done, true);
    assert.compareArray(log, ['get then false', 'get then false', 'get then true']);

    log.length = 0;
    await thrower().next().then(function () {
      throw new Test262Error('thrower must reject');
    }, function (error) {
      assert.sameValue(error.message, 'boom');
    });
    assert.compareArray(log, [], 'a rejection has no iterator result to inspect');
  } finally {
    delete Object.prototype.then;
  }

  Object.defineProperty(Object.prototype, 'then', {
    configurable: true,
    value: function (resolve) {
      seen = this;
      resolve('assimilated');
    }
  });
  var viaThenable;
  try {
    viaThenable = await yielder().next();
  } finally {
    delete Object.prototype.then;
  }
  assert.sameValue(viaThenable, 'assimilated', 'a callable then assimilates the request result');
  assert.sameValue(seen.value, 'a');
  assert.sameValue(seen.done, false);

  Object.defineProperty(Object.prototype, 'then', {
    configurable: true,
    get: function () { throw new Test262Error('then getter'); }
  });
  var viaThrowingGetter;
  try {
    viaThrowingGetter = await yielder().next().then(
      function () { return 'fulfilled'; },
      function (error) { return error.constructor === Test262Error; }
    );
  } finally {
    delete Object.prototype.then;
  }
  assert.sameValue(viaThrowingGetter, true, 'an abrupt Get(then) rejects the request promise');
});
