// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-%typedarray%.from
description: >
  The target typed array and the values collected from the source stay
  reachable while %TypedArray%.from runs user code (mapfn, valueOf) that
  triggers a garbage collection.
info: |
  %TypedArray%.from ( source [ , mapfn [ , thisArg ] ] )

  6. If usingIterator is not undefined, then
    a. Let values be ? IteratorToList(? GetIteratorFromMethod(source, usingIterator)).
    c. Let targetObj be ? TypedArrayCreateFromConstructor(C, « 𝔽(len) »).
    e. Repeat, while k < len,
      iii. If mapping is true, then
        1. Let mappedValue be ? Call(mapfn, thisArg, « kValue, 𝔽(k) »).
      vi. Perform ? Set(targetObj, Pk, mappedValue, true).
features: [TypedArray, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
}

var values = [0, {
  valueOf: function() {
    collect();
    values.length = 0;
    return 100;
  }
}, 2];

var fromIterable = Int32Array.from(values);
assert.sameValue(fromIterable.length, 3, "iterable path keeps the target alive");
assert.sameValue(fromIterable[0], 0);
assert.sameValue(fromIterable[1], 100);
assert.sameValue(fromIterable[2], 2);

var arrayLike = {
  length: 3,
  0: 1,
  1: {
    valueOf: function() {
      collect();
      return 5;
    }
  },
  2: 3
};

var fromArrayLike = Int32Array.from(arrayLike);
assert.sameValue(fromArrayLike.length, 3, "array-like path keeps the target alive");
assert.sameValue(fromArrayLike[0], 1);
assert.sameValue(fromArrayLike[1], 5);
assert.sameValue(fromArrayLike[2], 3);

var mapped = Int32Array.from([1, 2, 3], function(v) {
  collect();
  return v * 2;
});
assert.sameValue(mapped.length, 3, "a collecting mapfn keeps the target alive");
assert.sameValue(mapped[2], 6);
