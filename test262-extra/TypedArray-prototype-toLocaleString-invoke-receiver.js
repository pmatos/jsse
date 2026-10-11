// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-%typedarray%.prototype.tolocalestring
description: >
  %TypedArray%.prototype.toLocaleString looks up each element's
  toLocaleString with the element itself (a Number or BigInt, not its wrapper
  object) as the [[Get]] receiver, as Array.prototype.toLocaleString does.
  Expected values are cross-checked with Node.
info: |
  23.2.3.31 %TypedArray%.prototype.toLocaleString ( [ reserved1 [ , reserved2 ] ] )
    This is a distinct method that implements the same algorithm as
    Array.prototype.toLocaleString ...

  23.1.3.32 Array.prototype.toLocaleString ( [ reserved1 [ , reserved2 ] ] )
    c. If element is neither undefined nor null, then
      i. Let S be ? ToString(? Invoke(element, "toLocaleString")).

  7.3.20 Invoke ( V, P [ , argumentsList ] )
    2. Let func be ? GetV(V, P).

  7.3.3 GetV ( V, P )
    1. Let O be ? ToObject(V).
    2. Return ? O.[[Get]](P, V).
features: [TypedArray, BigInt]
---*/

function checkReceiver(proto, ta, expectedType) {
  var receivers = [];
  var desc = Object.getOwnPropertyDescriptor(proto, "toLocaleString");
  Object.defineProperty(proto, "toLocaleString", {
    get: function () {
      "use strict";
      receivers.push(typeof this);
      return function () { return "x"; };
    },
    configurable: true
  });
  try {
    assert.sameValue(ta.toLocaleString(), "x,x", expectedType + " result");
  } finally {
    Object.defineProperty(proto, "toLocaleString", desc);
  }
  assert.sameValue(receivers.length, 2, expectedType + " lookups");
  assert.sameValue(receivers[0], expectedType, expectedType + " receiver of the first lookup");
  assert.sameValue(receivers[1], expectedType, expectedType + " receiver of the second lookup");
}

checkReceiver(Number.prototype, new Uint8Array([1, 2]), "number");
checkReceiver(BigInt.prototype, new BigInt64Array([1n, 2n]), "bigint");
