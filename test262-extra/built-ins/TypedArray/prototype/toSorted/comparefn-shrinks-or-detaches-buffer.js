// Copyright (C) 2026 the jsse authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-%typedarray%.prototype.tosorted
description: >
  The result of %TypedArray%.prototype.toSorted has the length read before
  comparefn runs, even when comparefn shrinks or detaches the source buffer.
info: |
  %TypedArray%.prototype.toSorted ( comparefn )

  4. Let len be TypedArrayLength(taRecord).
  5. Let A be ? TypedArrayCreateSameType(O, len).
  ...
  9. Let sortedList be ? SortIndexedProperties(O, len, SortCompare,
     read-through-holes).
  10. Let j be 0.
  11. Repeat, while j < len,
    a. Perform ! Set(A, ! ToString(𝔽(j)), sortedList[j], true).
includes: [testTypedArray.js, detachArrayBuffer.js, compareArray.js]
features: [TypedArray, change-array-by-copy, resizable-arraybuffer]
---*/

testWithTypedArrayConstructors(function (TA) {
  var bpe = TA.BYTES_PER_ELEMENT;
  var rab = new ArrayBuffer(4 * bpe, { maxByteLength: 4 * bpe });
  var ta = new TA(rab);
  ta[0] = 4; ta[1] = 3; ta[2] = 2; ta[3] = 1;

  var result = ta.toSorted(function (a, b) {
    if (rab.byteLength !== 2 * bpe) rab.resize(2 * bpe);
    return a - b;
  });
  assert.sameValue(ta.length, 2, "source was shrunk");
  assert.compareArray(Array.from(result), [1, 2, 3, 4], "shrunk source");

  var ab = new ArrayBuffer(4 * bpe);
  var ta2 = new TA(ab);
  ta2[0] = 4; ta2[1] = 3; ta2[2] = 2; ta2[3] = 1;
  var detached = false;
  var result2 = ta2.toSorted(function (a, b) {
    if (!detached) {
      $DETACHBUFFER(ab);
      detached = true;
    }
    return a - b;
  });
  assert.sameValue(ta2.length, 0, "source was detached");
  assert.compareArray(Array.from(result2), [1, 2, 3, 4], "detached source");
});
