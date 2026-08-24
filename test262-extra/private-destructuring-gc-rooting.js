/*---
description: >
  A private reference's receiver stays reachable from target evaluation through
  iterator or property access and the eventual destructuring-assignment write.
esid: sec-runtime-semantics-destructuringassignmentevaluation
features: [class, class-methods-private, Symbol.iterator]
info: |
  13.15.5.5 Runtime Semantics: IteratorDestructuringAssignmentEvaluation

  AssignmentElement : DestructuringAssignmentTarget Initializer?
    1. If DestructuringAssignmentTarget is not a pattern, let lRef be
       ? Evaluation of DestructuringAssignmentTarget.
    2. Step the iterator and obtain the assigned value.
    ...
    8. Return ? PutValue(lRef, v).

  AssignmentRestElement : ... DestructuringAssignmentTarget
    1. If DestructuringAssignmentTarget is not a pattern, let lRef be
       ? Evaluation of DestructuringAssignmentTarget.
    2. Collect the remaining iterator values into an Array.
    ...
    5. Return ? PutValue(lRef, A).

  13.15.5.6 Runtime Semantics: KeyedDestructuringAssignmentEvaluation

  AssignmentElement : DestructuringAssignmentTarget Initializer?
    1. If DestructuringAssignmentTarget is not a pattern, let lRef be
       ? Evaluation of DestructuringAssignmentTarget.
    2. Let v be ? GetV(value, propertyName).
    ...
    7. Return ? PutValue(lRef, rhsValue).

  In every form, the Reference produced before arbitrary user code must retain
  its temporary private receiver until PutValue dispatches to PrivateSet.
---*/

function collectingIterable(values) {
  return {
    [Symbol.iterator]: function () {
      var index = 0;
      return {
        next: function () {
          $262.gc();
          if (index === values.length) {
            return { done: true };
          }
          return { value: values[index++], done: false };
        },
      };
    },
  };
}

// AssignmentElement evaluates the private target before iterator.next().
var elementWrites = [];
var elementError;

class ArrayElementTarget {
  set #x(value) {
    elementWrites.push(value);
  }

  static assign() {
    [(new ArrayElementTarget()).#x] = collectingIterable([42]);
  }
}

try {
  ArrayElementTarget.assign();
} catch (error) {
  elementError = error;
}

// AssignmentRestElement retains the target while every remaining value is
// collected. A distinct setter throw proves PrivateSet was actually reached.
class SetterThrew extends Error {}

var restWrite;
var restError;

class ArrayRestTarget {
  set #x(value) {
    restWrite = value;
    throw new SetterThrew("array rest setter ran");
  }

  static assign() {
    [...(new ArrayRestTarget()).#x] = collectingIterable([1, 2]);
  }
}

try {
  ArrayRestTarget.assign();
} catch (error) {
  restError = error;
}

// KeyedDestructuringAssignmentEvaluation evaluates the private target before
// GetV invokes the source getter. Its returned object also forces arena reuse
// if collection swept the target receiver.
var propertyWrites = [];
var propertyError;

class ObjectPropertyTarget {
  set #x(value) {
    propertyWrites.push(value.marker);
  }

  static assign(source) {
    ({ item: (new ObjectPropertyTarget()).#x } = source);
  }
}

var source = {
  get item() {
    $262.gc();
    return { marker: 7 };
  },
};

try {
  ObjectPropertyTarget.assign(source);
} catch (error) {
  propertyError = error;
}

assert.sameValue(elementError, undefined, "array element assignment completes normally");
assert.sameValue(elementWrites.length, 1, "array element setter runs once");
assert.sameValue(elementWrites[0], 42, "array element setter receives the iterator value");

assert.sameValue(restError instanceof SetterThrew, true, "array rest setter throw propagates");
assert.sameValue(restWrite.length, 2, "array rest setter receives the collected array");
assert.sameValue(restWrite[0], 1, "array rest preserves the first iterator value");
assert.sameValue(restWrite[1], 2, "array rest preserves the second iterator value");

assert.sameValue(propertyError, undefined, "object property assignment completes normally");
assert.sameValue(propertyWrites.length, 1, "object property setter runs once");
assert.sameValue(propertyWrites[0], 7, "object property setter receives the getter value");
