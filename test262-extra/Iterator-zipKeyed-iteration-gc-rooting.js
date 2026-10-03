/*---
description: >
  Iterator.zipKeyed keeps its inner iterators, their next methods, the padding
  values and the values already read for the current result reachable across
  garbage collection while the helper is stepped, even when the helper is the
  only thing referencing them.
esid: sec-iterator.zipkeyed
info: |
  Iterator.zipKeyed ( iterables [ , options ] )

  ...
  15. Let closure be a new Abstract Closure with no parameters that captures
      iters, iterCount, padding, and mode and performs the following steps
      when called:
    ...
        6. Append value to results.
    ...
    iv. Let result be CreateObject(results, keys).

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
var zipped = Iterator.zipKeyed({
  a: makeIterator("a", 3, closeLog),
  b: makeIterator("b", 3, closeLog),
  c: makeIterator("c", 3, closeLog),
});

var out = [];
for (var step = zipped.next(); !step.done; step = zipped.next()) {
  collect();
  var result = step.value;
  out.push(
    result.a.tag + result.a.index +
    result.b.tag + result.b.index +
    result.c.tag + result.c.index
  );
}
assert.compareArray(out, ["a0b0c0", "a1b1c1", "a2b2c2"], "values survive collection while later iterators step");
assert.compareArray(closeLog, ["c", "b"], "shortest mode closes the open iterators after collection");

closeLog = [];
var longest = Iterator.zipKeyed(
  { x: makeIterator("x", 1, closeLog), y: makeIterator("y", 3, closeLog) },
  { mode: "longest", padding: { x: { tag: "pad-x" }, y: { tag: "pad-y" } } }
);
var seen = [];
for (var step2 = longest.next(); !step2.done; step2 = longest.next()) {
  collect();
  seen.push(step2.value.x.tag + "/" + step2.value.y.tag);
}
assert.compareArray(seen, ["x/y", "pad-x/y", "pad-x/y"], "padding values survive collection");

closeLog = [];
var held = Iterator.zipKeyed({
  p: makeIterator("p", 5, closeLog),
  q: makeIterator("q", 5, closeLog),
});
held.next();
collect();
held.next();
collect();
var returned = held.return();
assert.sameValue(returned.done, true, "return() reports done");
assert.compareArray(closeLog, ["q", "p"], "return() closes the original iterators in reverse order after collection");
