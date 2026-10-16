/*---
description: >
  Element stores and updates with a Number key (`o[i] = v`, `o[i] op= v`,
  `o[i]++`) observe the same semantics as the generic [[Set]] path on plain
  Arrays, TypedArrays, arguments objects, Proxies and ordinary objects, in
  a plain function body and in a generator body.
esid: sec-assignment-operators-runtime-semantics-evaluation
info: |
  PutValue ( V, W ) calls base.[[Set]](GetReferencedName(V), W, GetThisValue(V));
  a non-writable, frozen or inherited-setter target is honoured, a strict
  failure throws a TypeError, and TypedArray [[Set]] with a canonical numeric
  index never reaches the prototype chain.
  Postfix/Prefix Increment and Decrement run GetValue, ToNumeric on the old
  value exactly once, then PutValue with the incremented value.
features: [TypedArray, BigInt, Proxy, resizable-arraybuffer]
includes: [detachArrayBuffer.js, compareArray.js]
flags: [noStrict]
---*/

var cases = {};

cases.typedArrayStores = function () {
  var u8 = new Uint8Array(4);
  u8[0] = 257;
  u8[1] = -1;
  u8[2] = 3.9;
  assert.compareArray(Array.from(u8), [1, 255, 3, 0], "Uint8Array wraps");

  var c8 = new Uint8ClampedArray(2);
  c8[0] = 300;
  c8[1] = -5;
  assert.compareArray(Array.from(c8), [255, 0], "Uint8ClampedArray clamps");

  var i16 = new Int16Array(2);
  i16[1] = 40000;
  assert.sameValue(i16[1], -25536, "Int16Array wraps");

  var f32 = new Float32Array(1);
  f32[0] = 0.1;
  assert.sameValue(f32[0], Math.fround(0.1), "Float32Array rounds");

  var u8b = new Uint8Array(2);
  u8b[0] = "7";
  assert.sameValue(u8b[0], 7, "a string value is converted");
  u8b[1] = undefined;
  assert.sameValue(u8b[1], 0, "undefined converts to NaN, stored as 0");
  var f64 = new Float64Array(1);
  f64[0] = undefined;
  assert.sameValue(f64[0], NaN, "undefined stores NaN in a Float64Array");
};

cases.typedArrayOutOfRange = function () {
  var ta = new Uint8Array(2);
  ta[2] = 1;
  ta[-1] = 1;
  ta[1.5] = 1;
  ta[NaN] = 1;
  ta[Infinity] = 1;
  ta[4294967296] = 1;
  assert.sameValue(Object.keys(ta).length, 2, "no own property is created");
  assert.sameValue(ta.hasOwnProperty(2), false, "index 2");
  assert.sameValue(ta[2], undefined, "reads back undefined");
  assert.sameValue(ta[-1], undefined, "reads back undefined for -1");

  ta[-0] = 9;
  assert.sameValue(ta[0], 9, "ToPropertyKey(-0) is '0'");

  Object.prototype[5] = "inherited";
  try {
    ta[5] = 3;
    assert.sameValue(ta[5], undefined, "an out-of-range index never walks the prototype");
    assert.sameValue(ta.hasOwnProperty(5), false, "index 5");
  } finally {
    delete Object.prototype[5];
  }

  assert.sameValue((ta[7] = 11), 11, "the assignment expression yields the right-hand side");
  assert.sameValue(ta[7]++, NaN, "postfix on an absent element yields NaN");
  assert.sameValue(++ta[7], NaN, "prefix on an absent element yields NaN");
  ta[7] += 1;
  assert.sameValue(ta.hasOwnProperty(7), false, "no element was created");
};

cases.typedArrayStrictOutOfRange = function () {
  "use strict";
  var ta = new Uint8Array(1);
  ta[3] = 1;
  ta[-1] = 1;
  ta[3]++;
  ta[3] += 2;
  assert.sameValue(ta.length, 1, "a canonical numeric out-of-range store is a silent no-op");
  var frozenLike = new Uint8Array(1);
  assert.throws(TypeError, function () {
    Object.freeze(frozenLike);
  }, "freezing a non-empty TypedArray fails");
};

cases.typedArrayRhsOrdering = function () {
  var log = [];
  var ta = new Uint8Array(2);
  ta[(log.push("key"), 1)] = (log.push("rhs"), 5);
  assert.compareArray(log, ["key", "rhs"], "key before rhs");
  assert.sameValue(ta[1], 5);

  log = [];
  ta[0] = {
    valueOf: function () {
      log.push("valueOf");
      return 6;
    }
  };
  assert.compareArray(log, ["valueOf"], "ToNumber runs once on an object value");
  assert.sameValue(ta[0], 6);

  var detach = new Uint8Array(4);
  detach[1] = (
    $DETACHBUFFER(detach.buffer), 3
  );
  assert.sameValue(detach.length, 0, "detached by the right-hand side");
  assert.sameValue(detach[1], undefined, "the store after detach is a no-op");

  var rab = new ArrayBuffer(8, { maxByteLength: 16 });
  var view = new Uint8Array(rab);
  view[7] = (rab.resize(4), 9);
  assert.sameValue(view.length, 4, "shrunk by the right-hand side");
  assert.sameValue(view[7], undefined, "an index past the shrunk length is not stored");
  rab.resize(8);
  assert.sameValue(view[7], 0, "the shrunk store left no trace");

  var obj = { valueOf: function () { $DETACHBUFFER(ta.buffer); return 1; } };
  ta[0] = obj;
  assert.sameValue(ta.length, 0, "a valueOf that detaches makes the store a no-op");
};

cases.typedArrayCompound = function () {
  var ta = new Uint8Array([10, 20, 250]);
  ta[0] += 5;
  ta[1] -= 30;
  ta[2] += 10;
  assert.compareArray(Array.from(ta), [15, 246, 4], "compound arithmetic wraps");
  ta[0] <<= 1;
  ta[1] >>= 1;
  assert.compareArray(Array.from(ta), [30, 123, 4]);
  ta[2] **= 3;
  assert.sameValue(ta[2], 64);

  var log = [];
  ta[0] += {
    valueOf: function () {
      log.push("valueOf");
      return 1;
    }
  };
  assert.compareArray(log, ["valueOf"]);
  assert.sameValue(ta[0], 31);

  var detach = new Uint8Array([5]);
  var result = (detach[0] += ($DETACHBUFFER(detach.buffer), 1));
  assert.sameValue(result, 6, "the read happened before the right-hand side");
  assert.sameValue(detach.length, 0);

  var rab = new ArrayBuffer(4, { maxByteLength: 8 });
  var view = new Uint8Array(rab);
  view[3] = 2;
  view[3] += (rab.resize(2), 1);
  assert.sameValue(view.length, 2);
  rab.resize(4);
  assert.sameValue(view[3], 0, "the store after shrinking was dropped");
};

cases.typedArrayUpdate = function () {
  var ta = new Uint8Array([255, 0, 7]);
  assert.sameValue(ta[0]++, 255, "postfix yields the old value");
  assert.sameValue(ta[0], 0, "wraps");
  assert.sameValue(--ta[1], -1, "prefix yields the new, unwrapped value");
  assert.sameValue(ta[1], 255);
  assert.sameValue(++ta[2], 8);
  assert.sameValue(ta[2]--, 8);
  assert.sameValue(ta[2], 7);

  var f = new Float64Array([0.5]);
  f[0]++;
  assert.sameValue(f[0], 1.5);

  var big = new BigInt64Array([5n]);
  assert.sameValue(big[0]++, 5n, "BigInt postfix");
  assert.sameValue(++big[0], 7n, "BigInt prefix");
  big[0] += 1n;
  big[0] = 3n;
  assert.sameValue(big[0], 3n);
  assert.throws(TypeError, function () {
    big[0] = 1;
  }, "a Number is rejected by a BigInt array");
  assert.throws(TypeError, function () {
    big[0] += 1;
  }, "mixing BigInt and Number throws");
  assert.sameValue(big[0], 3n, "the failed stores changed nothing");

  var oob = new BigInt64Array(1);
  assert.throws(TypeError, function () {
    oob[4]++;
  }, "out-of-range update still runs ToBigInt(NaN)");
  oob[4] = 1n;
  assert.sameValue(oob.length, 1);

  var detach = new Uint8Array([1]);
  $DETACHBUFFER(detach.buffer);
  assert.sameValue(detach[0]++, NaN, "detached postfix");
  assert.sameValue(++detach[0], NaN, "detached prefix");
};

cases.typedArrayStringKeys = function () {
  var ta = new Uint8Array(3);
  ta["1"] = 4;
  assert.sameValue(ta[1], 4, "'1' is a canonical numeric index");
  ta["01"] = 5;
  assert.sameValue(ta[1], 4, "'01' is not canonical");
  assert.sameValue(ta["01"], 5, "'01' is an ordinary property");
  ta["1e3"] = 6;
  assert.sameValue(ta["1e3"], 6, "'1e3' is an ordinary property");
  ta["999999999999999"] = 1;
  ta["1000000000000000"] = 1;
  ta["-0"] = 1;
  ta["1.5"] = 1;
  assert.sameValue(ta.hasOwnProperty("999999999999999"), false, "15 digits, out of range");
  assert.sameValue(ta.hasOwnProperty("1000000000000000"), false, "16 digits, out of range");
  assert.sameValue(ta.hasOwnProperty("-0"), false, "'-0'");
  assert.sameValue(ta.hasOwnProperty("1.5"), false, "'1.5'");
  assert.compareArray(Object.keys(ta), ["0", "1", "2", "01", "1e3"]);
  assert.sameValue(Reflect.set(ta, "2", 8), true);
  assert.sameValue(Reflect.has(ta, "2"), true);
  assert.sameValue(Reflect.has(ta, "3"), false);
  assert.sameValue(Reflect.has(ta, "007"), false);
  assert.sameValue(Reflect.defineProperty(ta, "0", { value: 3 }), true);
  assert.sameValue(ta[0], 3);
  assert.sameValue(Reflect.defineProperty(ta, "3", { value: 3 }), false, "out-of-range define");
  for (var key in ta) {
    ta[key] += 1;
  }
  assert.compareArray(Array.from(ta), [4, 5, 9]);
};

cases.arrayStores = function () {
  var a = [1, 2, 3];
  a[1] = 20;
  a[3] = 4;
  a[0]++;
  a[2] += 7;
  assert.compareArray(a, [2, 20, 10, 4]);
  assert.sameValue(a.length, 4);

  var holes = [1, , 3];
  holes[1] = 2;
  assert.compareArray(holes, [1, 2, 3], "hole filled");
  var sparse = [1];
  sparse[3] = 4;
  assert.sameValue(sparse.length, 4);
  assert.sameValue(1 in sparse, false, "a gap stays a hole");

  var u = [1, 2];
  u[0] = undefined;
  assert.sameValue(0 in u, true, "a stored undefined is a present element");
  assert.sameValue(u.hasOwnProperty(0), true);
  u[0] += 1;
  assert.sameValue(u[0], NaN);
  var h = [];
  h[0]++;
  assert.sameValue(h[0], NaN, "update of an absent element writes NaN");
  assert.sameValue(0 in h, true);

  var objElem = [{ valueOf: function () { return 4; } }];
  assert.sameValue(objElem[0]++, 4, "ToNumeric result is the postfix value");
  assert.sameValue(objElem[0], 5);

  var neg = [1];
  neg[-1] = 2;
  neg[1.5] = 3;
  assert.sameValue(neg.length, 1);
  assert.sameValue(neg[-1], 2, "a negative index is an ordinary property");
  assert.sameValue(neg[1.5], 3);
  neg[-0] = 9;
  assert.sameValue(neg[0], 9, "-0 addresses element zero");

  var big = [];
  big[4294967294] = 1;
  assert.sameValue(big.length, 4294967295);
  big[4294967295] = 2;
  assert.sameValue(big.length, 4294967295, "2^32-1 is not an array index");
  assert.sameValue(big[4294967295], 2);

  var str = [1, 2];
  str["1"] = 5;
  str["2"] = 6;
  assert.compareArray(str, [1, 5, 6]);
};

cases.arrayFrozenAndReadOnly = function () {
  var frozen = Object.freeze([1, 2]);
  frozen[0] = 9;
  frozen[2] = 9;
  frozen[0]++;
  frozen[1] += 1;
  assert.compareArray(frozen, [1, 2], "a frozen Array ignores stores (sloppy)");
  assert.sameValue(frozen.length, 2);

  var sealed = Object.seal([1, 2]);
  sealed[0] = 5;
  sealed[2] = 5;
  assert.compareArray(sealed, [5, 2], "sealed: overwrite works, append does not");

  var noExt = Object.preventExtensions([1]);
  noExt[1] = 2;
  noExt[0] = 3;
  assert.compareArray(noExt, [3]);

  var ro = [1, 2];
  Object.defineProperty(ro, 0, { writable: false });
  ro[0] = 9;
  ro[0]++;
  ro[1] = 8;
  assert.compareArray(ro, [1, 8], "a non-writable element is untouched");

  var roLen = [1, 2];
  Object.defineProperty(roLen, "length", { writable: false });
  roLen[2] = 3;
  roLen[1] = 5;
  assert.sameValue(roLen.length, 2);
  assert.sameValue(roLen[2], undefined, "append refused by a read-only length");
  assert.sameValue(roLen[1], 5, "overwrite still works");
};

cases.arrayFrozenStrict = function () {
  "use strict";
  var frozen = Object.freeze([1, 2]);
  assert.throws(TypeError, function () {
    frozen[0] = 9;
  }, "store");
  assert.throws(TypeError, function () {
    frozen[2] = 9;
  }, "append");
  assert.throws(TypeError, function () {
    frozen[0]++;
  }, "update");
  assert.throws(TypeError, function () {
    frozen[1] += 1;
  }, "compound");
  var roLen = [1];
  Object.defineProperty(roLen, "length", { writable: false });
  assert.throws(TypeError, function () {
    roLen[1] = 2;
  }, "append past a read-only length");
  assert.compareArray(frozen, [1, 2]);
};

cases.arrayInheritedSetter = function () {
  var calls = 0;
  var lastValue;
  var lastReceiverWasTarget;
  Object.defineProperty(Array.prototype, 3, {
    set: function (v) {
      calls++;
      lastValue = v;
      lastReceiverWasTarget = this === a;
    },
    get: function () {
      return "inherited";
    },
    configurable: true
  });
  try {
    var a = [1, 2, 3];
    a[3] = 4;
    assert.sameValue(calls, 1, "the inherited setter ran");
    assert.sameValue(lastValue, 4);
    assert.sameValue(lastReceiverWasTarget, true, "with the array as receiver");
    assert.sameValue(a.length, 3, "no element was created");
    assert.sameValue(a.hasOwnProperty(3), false);
    a[3] += 1;
    assert.sameValue(calls, 2, "compound assignment reads then sets through the accessor");
    assert.sameValue(lastValue, "inherited1");
    a[3]++;
    assert.sameValue(calls, 3);
    assert.sameValue(lastValue, NaN);
    var holey = [1, , 3, , 5];
    holey[3] = 9;
    assert.sameValue(calls, 4, "a hole in front of an inherited setter");
    assert.sameValue(holey.hasOwnProperty(3), false);
    holey[1] = 2;
    assert.sameValue(holey[1], 2, "an unshadowed hole is filled");
    assert.sameValue(calls, 4);
  } finally {
    delete Array.prototype[3];
  }

  Array.prototype[1] = "proto";
  try {
    var b = [0, , 2];
    assert.sameValue(b[1], "proto");
    b[1] += "!";
    assert.sameValue(b[1], "proto!", "the read saw the inherited value");
    assert.sameValue(b.hasOwnProperty(1), true, "and the write created an own element");
  } finally {
    delete Array.prototype[1];
  }
};

cases.arrayUpdateOnceAndRevalidated = function () {
  var calls = 0;
  var arr = [
    {
      valueOf: function () {
        calls++;
        arr.length = 0;
        return 1;
      }
    }
  ];
  var result = arr[0]++;
  assert.sameValue(calls, 1, "ToNumeric ran once");
  assert.sameValue(result, 1);
  assert.sameValue(arr.length, 1, "the store re-created the element");
  assert.sameValue(arr[0], 2);

  calls = 0;
  var frozenInside = [
    {
      valueOf: function () {
        calls++;
        Object.freeze(frozenInside);
        return 10;
      }
    }
  ];
  var old = frozenInside[0];
  frozenInside[0]++;
  assert.sameValue(calls, 1);
  assert.sameValue(frozenInside[0], old, "the frozen element kept its object");

  var setterInside = [
    {
      valueOf: function () {
        Object.defineProperty(setterInside, 0, {
          set: function (v) {
            setterInside.seen = v;
          },
          get: function () {
            return 0;
          },
          configurable: true
        });
        return 3;
      }
    }
  ];
  setterInside[0]++;
  assert.sameValue(setterInside.seen, 4, "the new accessor received the store");

  var shrink = [1, 2, 3];
  shrink[2] += (shrink.length = 1, 5);
  assert.sameValue(shrink.length, 3, "the compound store re-grew the array");
  assert.sameValue(shrink[2], 8, "using the value read before the right-hand side");
  assert.sameValue(1 in shrink, false);

  var freezeInRhs = [1, 2];
  freezeInRhs[0] = (Object.freeze(freezeInRhs), 9);
  assert.sameValue(freezeInRhs[0], 1, "freezing in the right-hand side blocks the store");
  var freezeInCompound = [1, 2];
  freezeInCompound[0] += (Object.freeze(freezeInCompound), 9);
  assert.sameValue(freezeInCompound[0], 1);
};

cases.argumentsObject = function () {
  function mapped(a, b) {
    arguments[0] = 5;
    arguments[1]++;
    var seen = [a, b];
    b += 10;
    arguments[1] += 1;
    return seen.concat(arguments[1], arguments[2]);
  }
  assert.compareArray(mapped(1, 2, 3), [5, 3, 14, 3]);

  function extra() {
    arguments[2] = "x";
    arguments[3] = "y";
    arguments[0]++;
    return [arguments.length, arguments[0], arguments[2], arguments[3]];
  }
  assert.compareArray(extra(1, 2), [2, 2, "x", "y"]);

  function strictArgs(a) {
    "use strict";
    arguments[0] = 9;
    arguments[0]++;
    return [a, arguments[0]];
  }
  assert.compareArray(strictArgs(1), [1, 10], "an unmapped arguments object is detached");
};

cases.proxyBase = function () {
  var log = [];
  var target = [1, 2, 3];
  var p = new Proxy(target, {
    get: function (t, k, r) {
      log.push("get:" + String(k));
      return Reflect.get(t, k, r);
    },
    set: function (t, k, v, r) {
      log.push("set:" + String(k) + "=" + String(v));
      return Reflect.set(t, k, v, r);
    },
    defineProperty: function (t, k, d) {
      log.push("define:" + String(k));
      return Reflect.defineProperty(t, k, d);
    },
    getOwnPropertyDescriptor: function (t, k) {
      log.push("gopd:" + String(k));
      return Reflect.getOwnPropertyDescriptor(t, k);
    }
  });
  p[0] = 10;
  assert.sameValue(log[0], "set:0=10", "the trap sees a string key");
  log.length = 0;
  p[1]++;
  assert.sameValue(log[0], "get:1");
  assert.sameValue(log[1], "set:1=3");
  log.length = 0;
  p[2] += 4;
  assert.sameValue(log[0], "get:2");
  assert.sameValue(log[1], "set:2=7");
  assert.compareArray(target, [10, 3, 7]);

  var denied = new Proxy([], {
    set: function () {
      return false;
    }
  });
  denied[0] = 1;
  assert.sameValue(denied.length, 0, "a refusing set trap is a silent no-op (sloppy)");
  assert.throws(TypeError, function () {
    "use strict";
    denied[0] = 1;
  }, "and a TypeError in strict code");

  var protoProxy = Object.create(
    new Proxy({}, {
      set: function (t, k, v, r) {
        log.push("protoSet:" + k);
        return true;
      }
    })
  );
  log.length = 0;
  protoProxy[3] = 1;
  assert.compareArray(log, ["protoSet:3"], "a Proxy in the prototype chain intercepts");
  assert.sameValue(protoProxy.hasOwnProperty(3), false);

  var ta = new Uint8Array(2);
  var viaProto = Object.create(ta);
  viaProto[0] = 7;
  assert.sameValue(ta[0], 0, "a different receiver is not written through");
  assert.sameValue(viaProto.hasOwnProperty(0), true, "the receiver gets the data property");
  assert.sameValue(viaProto[0], 7);
  viaProto[5] = 7;
  assert.sameValue(viaProto.hasOwnProperty(5), false, "out of range on a different receiver is a no-op");
};

cases.ordinaryObjects = function () {
  var log = [];
  var proto = {
    set 0(v) {
      log.push("set0:" + v);
    },
    get 0() {
      return 41;
    }
  };
  var o = Object.create(proto);
  o[0] = 1;
  assert.compareArray(log, ["set0:1"]);
  o[0]++;
  assert.compareArray(log, ["set0:1", "set0:42"], "update reads the getter then calls the setter");
  o[0] += 2;
  assert.compareArray(log, ["set0:1", "set0:42", "set0:43"]);
  assert.sameValue(o.hasOwnProperty(0), false);

  var plain = {};
  plain[2] = 1;
  plain[2]++;
  plain[3] = plain[2] + 1;
  plain[3] *= 5;
  assert.sameValue(plain[2], 2);
  assert.sameValue(plain[3], 15);
  assert.compareArray(Object.keys(plain), ["2", "3"]);

  var ro = Object.create({}, { 0: { value: 1, writable: false, configurable: true } });
  ro[0] = 2;
  ro[0]++;
  assert.sameValue(ro[0], 1, "a non-writable own element is untouched");
  var inheritedRo = Object.create(ro);
  inheritedRo[0] = 5;
  assert.sameValue(inheritedRo.hasOwnProperty(0), false, "inherited non-writable blocks creation");
};

cases.primitiveAndNullishBases = function () {
  var s = "abc";
  s[0] = "x";
  s[5] = "x";
  s[0]++;
  s[1] += "!";
  assert.sameValue(s, "abc", "sloppy stores to a string primitive are ignored");

  assert.throws(TypeError, function () {
    "use strict";
    "abc"[0] = "x";
  }, "strict store to a string index");
  assert.throws(TypeError, function () {
    "use strict";
    "abc"[7] = "x";
  }, "strict store past a string's length");
  assert.throws(TypeError, function () {
    "use strict";
    (5)[0] = 1;
  }, "strict store to a number primitive");

  var log = [];
  assert.throws(TypeError, function () {
    null[0] = (log.push("rhs"), 1);
  }, "null base");
  assert.compareArray(log, ["rhs"], "the right-hand side ran first");
  assert.throws(TypeError, function () {
    var u;
    u[0] = 1;
  }, "undefined base");
  assert.throws(TypeError, function () {
    var u;
    u[0]++;
  }, "undefined base update");
  assert.throws(TypeError, function () {
    var n = null;
    n[1] += 1;
  }, "null base compound");

  Number.prototype[3] = 10;
  try {
    var five = 5;
    assert.sameValue(five[3]++, 10, "the old value of a primitive's wrapper is read");
    five[3] += 1;
  } finally {
    delete Number.prototype[3];
  }
};

cases.keyConversionOrder = function () {
  var log = [];
  var key = {
    toString: function () {
      log.push("toString");
      return "1";
    }
  };
  var arr = [1, 2, 3];
  arr[key] = (log.push("rhs"), 9);
  assert.compareArray(log, ["rhs", "toString"], "ToPropertyKey runs after the right-hand side");
  assert.compareArray(arr, [1, 9, 3]);

  log = [];
  arr[key] += (log.push("rhs"), 1);
  assert.sameValue(log[0], "toString", "compound converts the key before the right-hand side");
  assert.sameValue(log.indexOf("rhs") > 0, true);
  assert.sameValue(arr[1], 10);

  log = [];
  arr[key]++;
  assert.sameValue(log[0], "toString", "update converts the key");
  assert.sameValue(arr[1], 11);

  log = [];
  assert.throws(TypeError, function () {
    var n = null;
    n[key] = (log.push("rhs"), 1);
  });
  assert.sameValue(log[0], "rhs", "the right-hand side ran before the failing PutValue");
};

cases.resultValues = function () {
  var a = [1, 2];
  assert.sameValue((a[0] = 5), 5);
  assert.sameValue((a[0] += 3), 8);
  assert.sameValue((a[0] -= 1), 7);
  assert.sameValue(a[1]++, 2);
  assert.sameValue(++a[1], 4);
  assert.sameValue(a[1]--, 4);
  assert.sameValue(--a[1], 2);
  var ta = new Uint8Array(1);
  assert.sameValue((ta[0] = 300), 300, "the expression yields the unconverted value");
  assert.sameValue((ta[0] += 300), 344, "compound yields the unconverted sum");
  var s = [1];
  s[0] = "str";
  assert.sameValue((s[0] += 1), "str1");
};

function bodyOf(fn) {
  var src = fn.toString();
  return src.slice(src.indexOf("{") + 1, src.lastIndexOf("}"));
}

var GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;

Object.keys(cases).forEach(function (name) {
  var fn = cases[name];
  try {
    fn();
  } catch (e) {
    throw new Test262Error("function body: " + name + ": " + e);
  }
  try {
    var gen = new GeneratorFunction("assert", "$DETACHBUFFER", "Test262Error", bodyOf(fn) + "\nyield 1;");
    var it = gen(assert, $DETACHBUFFER, Test262Error);
    it.next();
  } catch (e) {
    throw new Test262Error("generator body: " + name + ": " + e);
  }
});
