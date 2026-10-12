/*---
description: >
  Promise combinators retain a resolve function returned only by a getter
  while acquiring or enumerating their input.
esid: sec-getpromiseresolve
info: |
  GetPromiseResolve obtains a callable before GetIterator or keyed input
  enumeration runs user code. That callable must remain available for every
  element even when input access requests a collection.
features: [await-dictionary, Symbol.iterator, Proxy, host-gc-required]
---*/

['all', 'allSettled', 'race', 'any', 'allKeyed', 'allSettledKeyed'].forEach(function (method) {
  var events = [];
  var outcome;
  var resolveCalls = 0;
  var result;
  function C(executor) {
    events.push('construct');
    result = this;
    executor(function (value) {
      outcome = { value: value };
    }, function (error) {
      outcome = { error: error };
    });
  }
  Object.defineProperty(C, 'resolve', {
    get: function () {
      events.push('resolve');
      return function (value) {
        assert.sameValue(this, C, method + ' resolve receiver');
        resolveCalls++;
        return { then: function (onFulfilled) { onFulfilled(value); } };
      };
    }
  });

  var input;
  if (method === 'allKeyed' || method === 'allSettledKeyed') {
    input = new Proxy({ entry: 17 }, {
      ownKeys: function () {
        events.push('input');
        $262.gc();
        return ['entry'];
      }
    });
  } else {
    input = {};
    input[Symbol.iterator] = function () {
      events.push('input');
      $262.gc();
      var done = false;
      return { next: function () {
        if (done) return { done: true };
        done = true;
        return { done: false, value: 17 };
      } };
    };
  }

  assert.sameValue(Promise[method].call(C, input), result, method + ' capability identity');
  assert.compareArray(events, ['construct', 'resolve', 'input'], method + ' setup order');
  assert.sameValue(resolveCalls, 1, method + ' retained resolve');
  assert.sameValue('error' in outcome, false, method + ' did not reject');
  var value = outcome.value;
  if (method === 'all' || method === 'allSettled') value = value[0];
  if (method === 'allKeyed' || method === 'allSettledKeyed') value = value.entry;
  if (method === 'allSettled' || method === 'allSettledKeyed') {
    assert.sameValue(value.status, 'fulfilled', method + ' status');
    value = value.value;
  }
  assert.sameValue(value, 17, method + ' resolved value');
});
