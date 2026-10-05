// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-object.fromentries
description: >
  The result object under construction, the iterator record, and each
  entry's key/value extracted mid-iteration stay reachable across the
  garbage collections that can happen while AddEntriesFromIterable runs
  arbitrary user code (GetIterator, next(), the "0"/"1" accessor getters,
  and ToPropertyKey's toString/valueOf calls).
info: |
  Object.fromEntries ( iterable )

  1. Let obj be OrdinaryObjectCreate(%Object.prototype%).
  ...
  5. Return ? AddEntriesFromIterable(obj, iterable, adder).

  AddEntriesFromIterable ( target, iterable, adder )

  1. Let iteratorRecord be ? GetIterator(iterable, sync).
  2. Repeat,
    a. Let next be ? IteratorStepValue(iteratorRecord).
    b. If next is done, return target.
    c. If next is not an Object, throw a TypeError exception.
    d. Let k be Completion(Get(next, "0")).
    e. IfAbruptCloseIterator(k, iteratorRecord).
    f. Let v be Completion(Get(next, "1")).
    g. IfAbruptCloseIterator(v, iteratorRecord).
    h. Let status be Completion(Call(adder, undefined, « k, v »)).
    i. IfAbruptCloseIterator(status, iteratorRecord).

  The adder calls ? ToPropertyKey(k), which for a non-primitive key invokes
  ? ToPrimitive(key, string) — a further user-code call site. Every call
  above can trigger a collection; the result object, the iterator, and the
  per-entry key/value locals must all still be reachable afterward.
features: [host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeEntry(label) {
  return {
    get '0'() {
      return {
        toString: function () {
          collect();
          return label + ' key';
        },
      };
    },
    get '1'() {
      collect();
      var value = {};
      value.label = label + ' value';
      return value;
    },
  };
}

var iterable = {
  [Symbol.iterator]: function () {
    collect();
    var count = 0;
    return {
      next: function () {
        if (count === 0) {
          ++count;
          return { done: false, value: makeEntry('first') };
        } else if (count === 1) {
          ++count;
          return { done: false, value: makeEntry('second') };
        }
        return { done: true };
      },
    };
  },
};

var result = Object.fromEntries(iterable);

assert.compareArray(Object.keys(result), ['first key', 'second key']);
assert.sameValue(result['first key'].label, 'first value');
assert.sameValue(result['second key'].label, 'second value');
