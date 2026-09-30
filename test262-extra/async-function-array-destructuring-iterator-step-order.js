/*---
description: >
  IteratorStepValue for each array-pattern element runs in strict left-to-
  right source order, and a later element's step happens only *after* an
  earlier element's suspended default resumes -- the iterator is never
  pre-stepped past the point the function has actually reached (issue
  #725). Also covers that a present element's own step still happens
  (its default is skipped, but IteratorStepValue itself is not).
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  IteratorBindingInitialization

  BindingElementList : BindingElisionElement

  1. Perform ? IteratorBindingInitialization for BindingElisionElement
     using iteratorRecord and environment as arguments.

  BindingElement : SingleNameBinding
  ...
  2. Let v be undefined.
  3. If iteratorRecord.[[Done]] is false, then
    a. Let next be Completion(IteratorStepValue(iteratorRecord)).
    ...
  4. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

function run() {
  var log = [];
  var values = [undefined, 99];
  var it = {};
  it[Symbol.iterator] = function () {
    var i = 0;
    return {
      next: function () {
        log.push('step' + i);
        var value = values[i];
        return { value: value, done: ++i > values.length };
      },
    };
  };

  async function f() {
    var [a = await Promise.resolve('default-a'), b = 2] = it;
    log.push('a=' + a + ',b=' + b);
  }

  var p = f();
  log.push('sync-end');
  return p.then(function () { return log; });
}

run().then(function (log) {
  assert.compareArray(
    log,
    ['step0', 'sync-end', 'step1', 'a=default-a,b=99'],
    'the second element steps only after the first default resumes, and a ' +
      'present value still steps (but skips its own default)'
  );
}).then($DONE, $DONE);
