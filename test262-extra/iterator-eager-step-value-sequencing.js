// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-iteratorstepvalue
description: >
  Eager Iterator helpers use the captured next method and read done before value,
  preserving errors without closing when a step fails, including across GC.
info: |
  GetIteratorDirect captures next once. IteratorStepValue calls that method,
  checks the result object, reads done, and reads value only when done is false.
  A failure in any of these steps propagates without IteratorClose.
includes: [compareArray.js]
features: [iterator-helpers, iterator-includes, host-gc-required]
---*/

var consumers = [
  function (it) { assert.compareArray(it.toArray(), [1, 2]); },
  function (it) {
    var values = [];
    it.forEach(function (value, index) {
      $262.gc();
      assert.sameValue(index, values.length);
      values.push(value);
    });
    assert.compareArray(values, [1, 2]);
  },
  function (it) { assert.sameValue(it.some(function () { return false; }), false); },
  function (it) { assert.sameValue(it.every(function () { return true; }), true); },
  function (it) { assert.sameValue(it.find(function () { return false; }), undefined); },
  function (it) { assert.sameValue(it.includes(99, 1), false); },
  function (it) { assert.sameValue(it.reduce(function (a, b) { return a + b; }, 0), 3); },
  function (it) { assert.sameValue(it.reduce(function (a, b) { return a + b; }), 3); }
];

consumers.forEach(function (consume) {
  var events = [];
  var nextGets = 0;
  var calls = 0;
  var closed = false;
  var it = {
    __proto__: Iterator.prototype,
    get next() {
      nextGets++;
      return function () {
        assert.sameValue(this, it);
        assert.sameValue(arguments.length, 0);
        var index = calls++;
        events.push('next' + index);
        return {
          get done() {
            events.push('done' + index);
            Object.defineProperty(it, 'next', {
              value: function () { throw new Test262Error('next was reacquired'); },
              configurable: true
            });
            $262.gc();
            return index === 2 ? 'finished' : false;
          },
          get value() {
            events.push('value' + index);
            $262.gc();
            if (index === 2) throw new Test262Error('exhausted value was read');
            return index + 1;
          }
        };
      };
    },
    return: function () { closed = true; return {}; }
  };
  consume(it);
  assert.sameValue(nextGets, 1, 'next is captured once');
  assert.sameValue(closed, false, 'exhaustion does not close');
  assert.compareArray(events, [
    'next0', 'done0', 'value0',
    'next1', 'done1', 'value1',
    'next2', 'done2'
  ]);

  ['next', 'done', 'value', 'nonobject'].forEach(function (stage) {
    var marker = {};
    var failureClosed = false;
    var failureIterator = {
      __proto__: Iterator.prototype,
      next: function () {
        if (stage === 'next') throw marker;
        if (stage === 'nonobject') return null;
        return {
          get done() {
            $262.gc();
            if (stage === 'done') throw marker;
            return false;
          },
          get value() { $262.gc(); throw marker; }
        };
      },
      return: function () { failureClosed = true; return {}; }
    };
    var thrown;
    try { consume(failureIterator); } catch (error) { thrown = error; }
    if (stage === 'nonobject') {
      assert(thrown instanceof TypeError, 'non-object result throws TypeError');
    } else {
      assert.sameValue(thrown, marker, stage + ' error identity');
    }
    assert.sameValue(failureClosed, false, stage + ' failure must not close');
  });
});
