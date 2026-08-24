/*---
description: >
  Object assignment destructuring keeps its source object reachable while
  computed property names and source getters run user code.
esid: sec-runtime-semantics-destructuringassignmentevaluation
info: |
  Runtime Semantics: DestructuringAssignmentEvaluation

  ObjectAssignmentPattern : { AssignmentPropertyList }
    1. Perform ? RequireObjectCoercible(value).
    2. Perform ? PropertyDestructuringAssignmentEvaluation of
       AssignmentPropertyList with argument value.

  Runtime Semantics: PropertyDestructuringAssignmentEvaluation

  AssignmentProperty : PropertyName : AssignmentElement
    1. Let name be ? Evaluation of PropertyName.
    2. Perform ? KeyedDestructuringAssignmentEvaluation of AssignmentElement
       with arguments value and name.

  The same value participates in every property-name evaluation and GetV for
  the complete pattern. JSSE's ToObject representation of that value must
  therefore survive user code for both object and primitive sources.
---*/

var objectComputed;
({
  [{
    toString: function () {
      $262.gc();
      return "value";
    },
  }]: objectComputed,
} = { value: 1 });
assert.sameValue(
  objectComputed,
  1,
  "an object source survives computed property-key coercion"
);

var primitiveComputed;
({
  [{
    toString: function () {
      $262.gc();
      return "1";
    },
  }]: primitiveComputed,
} = "ab");
assert.sameValue(
  primitiveComputed,
  "b",
  "a primitive source wrapper survives computed property-key coercion"
);

var objectGetterValue;
var objectAfterGetter;
({
  gc: objectGetterValue,
  after: objectAfterGetter,
} = {
  get gc() {
    $262.gc();
    return 2;
  },
  after: 3,
});
assert.sameValue(objectGetterValue, 2, "an object source getter returns its value");
assert.sameValue(
  objectAfterGetter,
  3,
  "an object source survives collection in an earlier getter"
);

Object.defineProperty(String.prototype, "destructuringGc", {
  configurable: true,
  get: function () {
    $262.gc();
    return "getter";
  },
});

var primitiveGetterValue;
var primitiveAfterGetter;
({
  destructuringGc: primitiveGetterValue,
  1: primitiveAfterGetter,
} = "ab");

delete String.prototype.destructuringGc;

assert.sameValue(
  primitiveGetterValue,
  "getter",
  "a getter receives the primitive source wrapper"
);
assert.sameValue(
  primitiveAfterGetter,
  "b",
  "a primitive source wrapper survives collection in an earlier getter"
);
