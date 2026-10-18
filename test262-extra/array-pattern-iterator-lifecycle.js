// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Lowered array binding and assignment patterns preserve step order, normal
  IteratorClose, and protocol-failure cleanup in all three suspension drivers.
esid: sec-iteratorstepvalue
info: |
  IteratorStepValue marks the iterator record done on a protocol failure.
  IteratorBindingInitialization and IteratorDestructuringAssignmentEvaluation
  close an underconsumed iterator on normal completion. A failed inner
  iterator does not prevent an enclosing for-of iterator from being closed.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-functions, async-iteration, generators, destructuring-binding, destructuring-assignment]
---*/

function makeRunner(kind, assignment, nestedLoop) {
  var suspend = (kind === 'sync' || kind === 'asyncYield') ? 'yield 7' : 'await Promise.resolve(7)';
  var pattern = assignment
    ? 'var a, b; [a = ' + suspend + ', b] = input;'
    : 'var [a = ' + suspend + ', b] = input;';
  var body = nestedLoop
    ? 'for (var item of outer) { ' + pattern + ' }'
    : pattern + ' return a;';
  var prefix = kind === 'sync' ? 'function*' :
    kind !== 'function' ? 'async function*' : 'async function';
  return Function('input', 'outer', 'return (' + prefix + '() {' + body + '})();');
}

async function complete(kind, result) {
  if (kind === 'function') return await result;
  var step = await result.next();
  if (kind === 'sync' || kind === 'asyncYield') {
    assert.sameValue(step.done, false);
    assert.sameValue(step.value, 7);
    step = await result.next(7);
  }
  assert.sameValue(step.done, true);
  return step.value;
}

asyncTest(async function () {
  var kinds = ['sync', 'asyncGenerator', 'asyncYield', 'function'];
  for (var kind of kinds) {
    for (var assignment of [false, true]) {
      var log = [];
      var input = {
        [Symbol.iterator]() {
          var index = 0;
          return {
            next() {
              log.push('next' + index++);
              return { value: undefined, done: false };
            },
            return() { log.push('return'); return {}; }
          };
        }
      };
      assert.sameValue(await complete(kind, makeRunner(kind, assignment, false)(input)), 7);
      assert.compareArray(log, ['next0', 'next1', 'return']);

      for (var failure of ['next', 'done', 'value']) {
        log = [];
        var error = new Test262Error(failure);
        input = {
          [Symbol.iterator]() {
            var index = 0;
            return {
              next() {
                if (index++ === 0) return { value: undefined, done: false };
                if (failure === 'next') throw error;
                return {
                  get done() { if (failure === 'done') throw error; return false; },
                  get value() { throw error; }
                };
              },
              return() { log.push('inner return'); return {}; }
            };
          }
        };
        var outer = {
          [Symbol.iterator]() {
            return {
              next() { return { value: 1, done: false }; },
              return() { log.push('outer return'); return {}; }
            };
          }
        };
        var caught;
        try {
          await complete(kind, makeRunner(kind, assignment, true)(input, outer));
        } catch (e) { caught = e; }
        assert.sameValue(caught, error);
        assert.compareArray(log, ['outer return']);
      }
    }
  }
});
