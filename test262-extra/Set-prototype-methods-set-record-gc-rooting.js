// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-getsetrecord
description: >
  The Set methods hold the Set record's has and keys functions, and the keys
  iterator's next method, for as long as they walk: when a getter returns a
  fresh function, the record or iterator record is the only thing holding it
  while later user code runs a garbage collection.
info: |
  GetSetRecord ( obj )
  ...
  7. Let has be ? Get(obj, "has").
  8. If IsCallable(has) is false, throw a TypeError exception.
  9. Let keys be ? Get(obj, "keys").
  10. If IsCallable(keys) is false, throw a TypeError exception.
  11. Return a new Set Record { [[SetObject]]: obj, [[Size]]: intSize, [[Has]]: has, [[Keys]]: keys }.

  GetIteratorFromMethod ( obj, method )
  ...
  3. Let nextMethod be ? Get(iterator, "next").
  4. Let iteratorRecord be the Iterator Record { [[Iterator]]: iterator, [[NextMethod]]: nextMethod, [[Done]]: false }.
features: [set-methods, host-gc-required]
---*/

function collect() {
  for (var i = 0; i < 1000; i++) {
    [{}, {}, {}, {}];
  }
  $262.gc();
}

var hasFromGetter = {
  size: 10,
  get has() {
    return function (value) {
      return value === 1;
    };
  },
  get keys() {
    collect();
    return function () {
      throw new Test262Error("keys should not be called");
    };
  }
};
assert.sameValue(new Set([1, 2]).isSubsetOf(hasFromGetter), false, "has survives the collection in the keys getter");

var steps = 0;
var nextFromGetter = {
  size: 1,
  has: function () {
    throw new Test262Error("has should not be called");
  },
  keys: function () {
    return {
      get next() {
        return function () {
          steps++;
          return {
            get done() {
              collect();
              return steps > 2;
            },
            value: steps
          };
        };
      }
    };
  }
};
var values = Array.from(new Set().union(nextFromGetter));
assert.sameValue(values.length, 2, "the keys iterator's next method survives each collection");
assert.sameValue(values[0], 1, "first value");
assert.sameValue(values[1], 2, "second value");
