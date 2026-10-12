// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getsetrecord
description: >
  GetSetRecord propagates an abrupt completion from ToNumber(rawSize) as is,
  and truncates the size before the sign check, so a size in (-1, 0) is -0
  and does not throw a RangeError.
info: |
  GetSetRecord ( obj )
  1. If obj is not an Object, throw a TypeError exception.
  2. Let rawSize be ? Get(obj, "size").
  3. Let numSize be ? ToNumber(rawSize).
  4. NOTE: If rawSize is undefined, then numSize will be NaN.
  5. If numSize is NaN, throw a TypeError exception.
  6. Let intSize be ! ToIntegerOrInfinity(numSize).
  7. If intSize < 0, throw a RangeError exception.
features: [set-methods]
includes: [compareArray.js]
---*/

var methods = [
  "union",
  "intersection",
  "difference",
  "symmetricDifference",
  "isSubsetOf",
  "isSupersetOf",
  "isDisjointFrom"
];

function setLike(size) {
  return {
    size: size,
    has: function () {
      return false;
    },
    keys: function () {
      return [].values();
    }
  };
}

methods.forEach(function (name) {
  var throwingSize = setLike({
    valueOf: function () {
      throw new Test262Error();
    }
  });
  assert.throws(Test262Error, function () {
    new Set([1])[name](throwingSize);
  }, name + " propagates the error thrown by ToNumber(rawSize)");

  assert.throws(TypeError, function () {
    new Set([1])[name](setLike(Symbol()));
  }, name + " propagates the TypeError of ToNumber on a Symbol size");

  new Set([1])[name](setLike(-0.5));
  new Set([1])[name](setLike(-0.99));

  assert.throws(RangeError, function () {
    new Set([1])[name](setLike(-1));
  }, name + " rejects a size that truncates below zero");
});

assert.compareArray(
  Array.from(new Set([1]).union(setLike(-0.5))),
  [1],
  "a size of -0.5 is -0 after ToIntegerOrInfinity"
);
