/*---
description: >
  A class declaration's static computed field key that delegates with
  `yield*` inside a generator body is a real generator-suspension point.
  Resuming past it must not re-fetch the delegate's iterator or replay
  next() calls already made -- the same single-evaluation contract every
  other `yield*` delegation site gets.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
info: |
  Runtime Semantics: ClassDefinitionEvaluation (15.7.14) evaluates each
  ClassElementName's computed PropertyName exactly once, in source order,
  as ordinary Evaluation in the same execution context as the enclosing
  generator.

  YieldExpression : yield * AssignmentExpression -- the yield-star
  delegation algorithm requests the delegate's iterator exactly once and
  calls its next() method exactly once per external .next() call
  thereafter.
features: [generators, class-static-fields-public, computed-property-names]
---*/

var nextCalls = 0;

function countingIterable(values) {
  return {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          nextCalls++;
          if (i < values.length) {
            return { value: values[i++], done: false };
          }
          return { value: undefined, done: true };
        },
      };
    },
  };
}

function* g() {
  class C extends null {
    static [yield* countingIterable(["a", "b"])] = 1;
  }
  return C;
}

var it = g();

var r1 = it.next();
assert.sameValue(r1.value, "a", "first delegated yield");
assert.sameValue(r1.done, false);
assert.sameValue(
  nextCalls,
  1,
  "the delegate iterator is advanced exactly once for the first external .next()"
);

var r2 = it.next();
assert.sameValue(r2.value, "b", "second delegated yield");
assert.sameValue(r2.done, false);
assert.sameValue(
  nextCalls,
  2,
  "resuming does not re-fetch the delegate iterator or replay the first next() call"
);

var r3 = it.next();
assert.sameValue(
  r3.done,
  true,
  "the generator completes once the delegate is exhausted"
);
assert.sameValue(
  typeof r3.value,
  "function",
  "the generator's return value is the now-fully-defined class"
);
assert.sameValue(
  nextCalls,
  3,
  "exactly one further next() call closes out the delegated iteration"
);
