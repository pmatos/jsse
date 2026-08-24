/*---
description: >
  A destructuring assignment's member target, its pending property key, and the
  values it assigns stay reachable across the iterator, getter, and
  ToPropertyKey user code that runs between target evaluation and PutValue.
esid: sec-runtime-semantics-destructuringassignmentevaluation
features: [Symbol.iterator]
info: |
  13.15.5.5 Runtime Semantics: IteratorDestructuringAssignmentEvaluation

  AssignmentElement : DestructuringAssignmentTarget Initializer?
    1. If DestructuringAssignmentTarget is not a pattern, let lRef be
       ? Evaluation of DestructuringAssignmentTarget.
    2. Step the iterator and obtain the assigned value.
    8. Return ? PutValue(lRef, v).

  AssignmentRestElement : ... DestructuringAssignmentTarget
    2. Repeat, collecting each remaining iterator value into A.
    5. Return ? PutValue(lRef, A).

  13.15.5.6 Runtime Semantics: KeyedDestructuringAssignmentEvaluation

  PutValue on a property Reference performs ToPropertyKey on the held key and
  then invokes the receiver's [[Set]]. Every value that participates -- the
  base, the un-coerced key, each already collected rest value, and the value
  being written -- must therefore survive the intervening user code.
---*/

function gcThenValue(value) {
  return function () {
    $262.gc();
    return value;
  };
}

// Rest collection must retain the values already taken from the iterator while
// later next() calls run user code.
var restSource = {
  [Symbol.iterator]: function () {
    var index = 0;
    return {
      next: function () {
        $262.gc();
        if (index === 3) {
          return { done: true };
        }
        return { value: { marker: { deep: index++ } }, done: false };
      },
    };
  },
};

var rest;
[...rest] = restSource;
assert.sameValue(rest.length, 3, "every iterator value is collected");
assert.sameValue(rest[0].marker.deep, 0, "the first collected value survives");
assert.sameValue(rest[1].marker.deep, 1, "the second collected value survives");
assert.sameValue(rest[2].marker.deep, 2, "the third collected value survives");

// A member target's base is reachable only through the Reference Record.
var elementWrites = [];
var elementProto = {
  set x(value) {
    elementWrites.push(value);
  },
};

var singleStep = {
  [Symbol.iterator]: function () {
    var taken = false;
    return {
      next: function () {
        $262.gc();
        if (taken) {
          return { done: true };
        }
        taken = true;
        return { value: "element", done: false };
      },
    };
  },
};

[Object.create(elementProto).x] = singleStep;
assert.sameValue(elementWrites.length, 1, "the array element base setter runs");
assert.sameValue(elementWrites[0], "element", "the array element setter receives the value");

var propertyWrites = [];
var propertyProto = {
  set y(value) {
    propertyWrites.push(value);
  },
};

({ p: Object.create(propertyProto).y } = { get p() { $262.gc(); return "property"; } });
assert.sameValue(propertyWrites.length, 1, "the object property base setter runs");
assert.sameValue(propertyWrites[0], "property", "the object property setter receives the value");

// The un-coerced key and the assigned value both outlive the user code that
// runs before and during ToPropertyKey.
var elementTarget = {};
[elementTarget[{ toString: gcThenValue("elementKey") }]] = {
  [Symbol.iterator]: function () {
    var taken = false;
    return {
      next: function () {
        $262.gc();
        if (taken) {
          return { done: true };
        }
        taken = true;
        return { value: { marker: 42 }, done: false };
      },
    };
  },
};
assert.sameValue(
  elementTarget.elementKey.marker,
  42,
  "the array element key and value survive ToPropertyKey"
);

var propertyTarget = {};
({ p: propertyTarget[{ toString: gcThenValue("propertyKey") }] } = {
  get p() {
    $262.gc();
    return { marker: 7 };
  },
});
assert.sameValue(
  propertyTarget.propertyKey.marker,
  7,
  "the object property key and value survive ToPropertyKey"
);
