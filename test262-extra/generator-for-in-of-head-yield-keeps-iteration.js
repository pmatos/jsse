/*---
description: >
  A yield in a for-in or for-of declaration head's binding initializer
  resumes the current iteration without repeating completed iterations.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation obtains the next value before performing binding
  initialization, then evaluates the loop body after that initialization.
  Suspending during binding initialization must preserve the loop's iterator.
features: [generators, destructuring-binding]
---*/

var steps = 0;
var starts = 0;
var iterable = {
  [Symbol.iterator]: function () {
    starts++;
    return {
      next: function () {
        steps++;
        return steps <= 2 ? { value: {}, done: false } : { done: true };
      }
    };
  }
};
var ofValues = [];
function* ofLoop() {
  for (const { a = yield 'head' } of iterable) {
    ofValues.push(a);
  }
}
var ofIterator = ofLoop();
assert.sameValue(ofIterator.next().value, 'head');
assert.sameValue(ofIterator.next('first').value, 'head');
assert.sameValue(ofIterator.next('second').done, true);
assert.sameValue(ofValues.length, 2, 'for-of body executes once per element');
assert.sameValue(ofValues[0], 'first');
assert.sameValue(ofValues[1], 'second');
assert.sameValue(starts, 1, 'for-of iterator is acquired once');
assert.sameValue(steps, 3, 'for-of iterator is exhausted once');

var inValues = [];
function* inLoop() {
  for (const { a = yield 'key' } in { x: 1, y: 2 }) {
    inValues.push(a);
  }
}
var inIterator = inLoop();
assert.sameValue(inIterator.next().value, 'key');
assert.sameValue(inIterator.next('x').value, 'key');
assert.sameValue(inIterator.next('y').done, true);
assert.sameValue(inValues.length, 2, 'for-in body executes once per key');
assert.sameValue(inValues[0], 'x');
assert.sameValue(inValues[1], 'y');
