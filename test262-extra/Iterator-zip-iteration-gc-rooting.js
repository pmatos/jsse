/*---
description: >
  Iterator.zip keeps its inner iterators, their next methods, the padding
  values and the values already read for the current tuple reachable across
  garbage collection while the helper is stepped, even when the helper is the
  only thing referencing them.
esid: sec-iterator.zip
info: |
  Iterator.zip ( iterables [ , options ] )

  ...
  16. Let closure be a new Abstract Closure with no parameters that captures
      iters, iterCount, padding, and mode and performs the following steps
      when called:
    ...
    c. Repeat, while finishResults is false,
      i. Let results be a new empty List.
      ii. For each integer i such that 0 <= i < iterCount, in ascending order,
        ...
          3. Let result be Completion(IteratorStepValue(iter)).
        ...
        6. Append value to results.

  The values appended to results are held only by the implementation while the
  remaining iterators run user code that can trigger a collection, and the
  iterator records must outlive every suspension of the helper.
includes: [compareArray.js]
features: [joint-iteration, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

function makeIterator(tag, count, closeLog) {
  var produced = 0;
  return {
    next: function () {
      collect();
      if (produced >= count) {
        return { done: true, value: undefined };
      }
      return { done: false, value: { tag: tag, index: produced++ } };
    },
    return: function () {
      closeLog.push(tag);
      return {};
    },
    [Symbol.iterator]: function () {
      return this;
    },
  };
}

var closeLog = [];
var zipped = Iterator.zip([
  makeIterator("a", 3, closeLog),
  makeIterator("b", 3, closeLog),
  makeIterator("c", 3, closeLog),
]);

var out = [];
for (var step = zipped.next(); !step.done; step = zipped.next()) {
  collect();
  var tuple = step.value;
  out.push(
    tuple[0].tag + tuple[0].index +
    tuple[1].tag + tuple[1].index +
    tuple[2].tag + tuple[2].index
  );
}
assert.compareArray(out, ["a0b0c0", "a1b1c1", "a2b2c2"], "values survive collection while later iterators step");
assert.compareArray(closeLog, ["c", "b"], "shortest mode closes the open iterators after collection");

closeLog = [];
var longest = Iterator.zip(
  [makeIterator("x", 1, closeLog), makeIterator("y", 3, closeLog)],
  { mode: "longest", padding: [{ tag: "pad-x" }, { tag: "pad-y" }] }
);
var seen = [];
for (var step2 = longest.next(); !step2.done; step2 = longest.next()) {
  collect();
  seen.push(step2.value[0].tag + "/" + step2.value[1].tag);
}
assert.compareArray(seen, ["x/y", "pad-x/y", "pad-x/y"], "padding values survive collection");

closeLog = [];
var held = Iterator.zip([
  makeIterator("p", 5, closeLog),
  makeIterator("q", 5, closeLog),
]);
held.next();
collect();
held.next();
collect();
var returned = held.return();
assert.sameValue(returned.done, true, "return() reports done");
assert.compareArray(closeLog, ["q", "p"], "return() closes the original iterators in reverse order after collection");
