/*---
description: >
  A rest element following an awaiting element is still lowered (the
  preceding element's default forces suspension, even though a rest
  element's own binding never contains a suspension) and collects the
  remaining iterator values into a fresh array after the awaited default
  resumes (issue #725).
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  IteratorBindingInitialization

  BindingRestElement : ... BindingIdentifier
  1. Let A be ! ArrayCreate(0).
  2. Let n be 0.
  3. Repeat,
    a. If iteratorRecord.[[Done]] is false, then
      i. Let next be Completion(IteratorStepValue(iteratorRecord)).
      ...
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

async function run() {
  var [a = await 1, ...rest] = [undefined, 2, 3, 4];
  return { a: a, rest: rest };
}

run().then(function (result) {
  assert.sameValue(result.a, 1, "the awaiting element's default is used");
  assert.compareArray(result.rest, [2, 3, 4], 'rest collects the remaining values');
}).then($DONE, $DONE);
