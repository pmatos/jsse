/*---
description: >
  Reading a hole or an index past the backing vector of an Array, and
  reading a property that an object does not own, observe the prototype chain
  exactly as [[Get]] specifies: inherited indices, accessors, Proxy traps and
  exotic receivers (TypedArray, String wrapper, mapped arguments) are honoured.
esid: sec-ordinary-object-internal-methods-and-internal-slots-get-p-receiver
info: |
  OrdinaryGet ( O, P, Receiver ) returns undefined only when O has no own
  property P and its [[Prototype]] is null or answers undefined; a hole is not
  an own property, so the prototype chain (including a Proxy [[Get]] trap or
  an index accessor on Array.prototype/Object.prototype) is consulted.
  A TypedArray never consults the chain for a canonical numeric key.
features: [Proxy, TypedArray]
includes: [compareArray.js]
flags: [noStrict]
---*/

function setIndex(obj, key, value) {
  Object.defineProperty(obj, key, { value: value, configurable: true, writable: true, enumerable: false });
}

// A hole and an out-of-range read are undefined when nothing inherits the index.
(function () {
  var a = new Array(4);
  assert.sameValue(a[0], undefined, "hole 0");
  assert.sameValue(a[3], undefined, "hole 3");
  assert.sameValue(a[4], undefined, "past length");
  assert.sameValue(a[5000], undefined, "far past length");
  assert.sameValue(a[4294967294], undefined, "largest index");
  assert.sameValue(a[4294967295], undefined, "2^32-1 is an ordinary key");
  assert.sameValue(a[2 ** 40], undefined, "huge index");
  var lit = [1, , 3];
  assert.sameValue(lit[1], undefined, "literal hole");
  assert.sameValue(lit[2], 3, "present element after a hole");
  assert.sameValue(lit[-0], 1, "-0 addresses element 0");
  assert.sameValue(0 in lit, true, "present");
  assert.sameValue(1 in lit, false, "hole stays absent after reads");
  assert.sameValue(lit.hasOwnProperty(1), false, "hole stays absent");
})();

// Array.prototype[i] is visible through holes and past the vector, and only there.
(function () {
  setIndex(Array.prototype, 3, "ap");
  try {
    var a = new Array(5);
    assert.sameValue(a[3], "ap", "hole sees Array.prototype[3]");
    assert.sameValue(a[2], undefined, "other hole");
    var b = [10, 20];
    assert.sameValue(b[3], "ap", "past the vector sees Array.prototype[3]");
    var c = [0, 1, 2, 3];
    assert.sameValue(c[3], 3, "present element wins");
    setIndex(Array.prototype, 2, "late");
    assert.sameValue(a[2], "late", "an index added after earlier reads is seen");
  } finally {
    delete Array.prototype[3];
    delete Array.prototype[2];
  }
  assert.sameValue(new Array(5)[3], undefined, "gone after delete");
})();

// Object.prototype[i] sits behind Array.prototype.
(function () {
  setIndex(Object.prototype, 5, "op");
  try {
    assert.sameValue(new Array(8)[5], "op", "hole");
    assert.sameValue([][5], "op", "empty array");
    assert.sameValue([1][5], "op", "past the vector");
    assert.sameValue(new Array(8)[4], undefined, "other hole");
    setIndex(Array.prototype, 5, "ap");
    assert.sameValue(new Array(8)[5], "ap", "Array.prototype shadows Object.prototype");
    delete Array.prototype[5];
  } finally {
    delete Object.prototype[5];
    delete Array.prototype[5];
  }
  assert.sameValue([][5], undefined, "gone after delete");
})();

// An inherited accessor runs with the array as receiver.
(function () {
  var receivers = {};
  var count = 0;
  Object.defineProperty(Array.prototype, 1, {
    get: function () { receivers[count++] = this; return "got"; },
    configurable: true,
  });
  try {
    var a = new Array(3);
    assert.sameValue(a[1], "got", "hole");
    assert.sameValue(receivers[0], a, "receiver is the array");
    var b = [];
    assert.sameValue(b[1], "got", "past the vector");
    assert.sameValue(receivers[1], b, "receiver is the second array");
    assert.sameValue(a[0], undefined, "unrelated hole");
    assert.sameValue(count, 2, "getter ran once per read");
  } finally {
    delete Array.prototype[1];
  }
  Object.defineProperty(Object.prototype, 0, {
    get: function () { return this; },
    configurable: true,
  });
  try {
    var c = [, 1];
    assert.sameValue(c[0], c, "Object.prototype getter sees the array");
  } finally {
    delete Object.prototype[0];
  }
  Object.defineProperty(Array.prototype, 0, { get: undefined, set: function () {}, configurable: true });
  try {
    assert.sameValue([, 1][0], undefined, "accessor without getter reads undefined");
    Object.prototype[0] = "op";
    assert.sameValue([, 1][0], undefined, "accessor on Array.prototype hides Object.prototype");
  } finally {
    delete Array.prototype[0];
    delete Object.prototype[0];
  }
})();

// Indices that are not array indices are ordinary string keys.
(function () {
  Object.prototype["4294967295"] = "big";
  Object.prototype[4294967294] = "max";
  try {
    var a = [];
    assert.sameValue(a[4294967295], "big", "2^32-1 reads the string key");
    assert.sameValue(a[4294967294], "max", "2^32-2 is the largest index");
    assert.sameValue(a["4294967295"], "big", "string form");
  } finally {
    delete Object.prototype["4294967295"];
    delete Object.prototype[4294967294];
  }
  Array.prototype["-1"] = "neg";
  Array.prototype["1.5"] = "frac";
  Array.prototype.NaN = "nan";
  try {
    var b = [7];
    assert.sameValue(b[-1], "neg", "negative");
    assert.sameValue(b[1.5], "frac", "fractional");
    assert.sameValue(b[NaN], "nan", "NaN");
    assert.sameValue(b[-0], 7, "-0 is element 0");
    assert.sameValue(b[0.5], undefined, "fraction not inherited");
  } finally {
    delete Array.prototype["-1"];
    delete Array.prototype["1.5"];
    delete Array.prototype.NaN;
  }
})();

// A present undefined element shadows an inherited index; a hole does not.
(function () {
  setIndex(Array.prototype, 1, "ap");
  try {
    var a = [0, undefined, 2];
    assert.sameValue(a[1], undefined, "literal undefined shadows");
    var b = [0, , 2];
    assert.sameValue(b[1], "ap", "hole does not");
    b[1] = undefined;
    assert.sameValue(b[1], undefined, "assigned undefined shadows");
    var c = new Array(3);
    c[1] = undefined;
    assert.sameValue(c[1], undefined, "assigned undefined shadows a hole");
    assert.sameValue(c[0], undefined, "neighbour hole");
    var d = Array(3).fill(undefined);
    assert.sameValue(d[1], undefined, "fill(undefined) creates present elements");
    assert.sameValue(1 in d, true, "fill(undefined) element is own");
    var e = [1, 2, 3];
    delete e[1];
    assert.sameValue(e[1], "ap", "deleted element sees the prototype");
    var f = [1, 2, 3];
    f.length = 1;
    assert.sameValue(f[1], "ap", "truncated element sees the prototype");
  } finally {
    delete Array.prototype[1];
  }
})();

// An own index accessor elsewhere in the array.
(function () {
  var a = new Array(4);
  Object.defineProperty(a, 1, { get: function () { return "own"; }, configurable: true });
  setIndex(Array.prototype, 2, "ap");
  try {
    assert.sameValue(a[1], "own", "own accessor");
    assert.sameValue(a[2], "ap", "hole next to an own accessor");
    assert.sameValue(a[3], undefined, "plain hole");
    assert.sameValue(a[9], undefined, "past the vector");
  } finally {
    delete Array.prototype[2];
  }
  var frozen = Object.freeze(new Array(3));
  setIndex(Object.prototype, 1, "op");
  try {
    assert.sameValue(frozen[1], "op", "frozen array hole");
  } finally {
    delete Object.prototype[1];
  }
})();

// A Proxy anywhere on the chain sees hole and out-of-range reads.
(function () {
  var log = [];
  var proxy = new Proxy({}, {
    get: function (target, key, receiver) {
      log.push(key);
      return key === "1" ? "px" : undefined;
    },
  });
  var a = new Array(3);
  Object.setPrototypeOf(a, proxy);
  assert.sameValue(a[1], "px", "hole reaches the trap");
  assert.sameValue(a[7], undefined, "past the vector reaches the trap");
  a[0] = "own";
  assert.sameValue(a[0], "own", "present element skips the trap");
  assert.compareArray(log, ["1", "7"], "trap saw only the absent indices");

  var saved = Object.getPrototypeOf(Array.prototype);
  var calls = 0;
  Object.setPrototypeOf(Array.prototype, new Proxy(Object.prototype, {
    get: function (target, key, receiver) { calls++; return Reflect.get(target, key, receiver); },
  }));
  try {
    assert.sameValue(new Array(2)[1], undefined, "Proxy behind Array.prototype");
    assert.sameValue(calls, 1, "trap ran for the hole read");
    assert.sameValue([1][4], undefined, "past the vector");
    assert.sameValue(calls, 2, "trap ran for the out-of-range read");
  } finally {
    Object.setPrototypeOf(Array.prototype, saved);
  }
  assert.sameValue(new Array(2)[1], undefined, "restored");
  assert.sameValue(calls, 2, "trap no longer runs");
})();

// Array subclasses and arrays with a replaced prototype.
(function () {
  class A extends Array {}
  var a = new A(3);
  assert.sameValue(a[1], undefined, "subclass hole");
  A.prototype[1] = "sub";
  assert.sameValue(a[1], "sub", "subclass prototype index");
  assert.sameValue(a[2], undefined, "other subclass hole");
  delete A.prototype[1];
  assert.sameValue(a[1], undefined, "removed again");

  var b = [1, , 3];
  Object.setPrototypeOf(b, { 1: "custom" });
  assert.sameValue(b[1], "custom", "custom ordinary prototype");
  assert.sameValue(b[5], undefined, "custom prototype lacks 5");
  Object.setPrototypeOf(b, null);
  assert.sameValue(b[1], undefined, "null prototype");
  assert.sameValue(b[0], 1, "present element");
})();

// A String wrapper as prototype exposes virtual indices.
(function () {
  var a = new Array(4);
  Object.setPrototypeOf(a, new String("xyz"));
  assert.sameValue(a[1], "y", "String wrapper index through a hole");
  assert.sameValue(a[6], undefined, "String wrapper out of range");
  assert.sameValue(a.length, 4, "own length wins");
  var s = new String("ab");
  assert.sameValue(s[1], "b", "own virtual index");
  assert.sameValue(s[2], undefined, "past the string");
  assert.sameValue(s.length, 2, "virtual length");
  Object.prototype[2] = "op";
  try {
    assert.sameValue(s[2], "op", "string wrapper miss reaches Object.prototype");
    assert.sameValue(s[1], "b", "virtual index wins");
  } finally {
    delete Object.prototype[2];
  }
})();

// TypedArrays never consult the chain for numeric keys.
(function () {
  setIndex(Object.prototype, 5, "op");
  setIndex(Array.prototype, 6, "ap");
  try {
    var u8 = new Uint8Array(2);
    assert.sameValue(u8[5], undefined, "out of range");
    assert.sameValue(u8[1], 0, "in range");
    assert.sameValue(u8[-1], undefined, "negative");
    assert.sameValue(u8["1.5"], undefined, "canonical fraction");
  } finally {
    delete Object.prototype[5];
    delete Array.prototype[6];
  }
})();

// Mapped and unmapped arguments objects.
(function () {
  function mapped(a, b) {
    a = "changed";
    return [arguments[0], arguments[1], arguments[2], arguments.length];
  }
  assert.compareArray(mapped(1), ["changed", undefined, undefined, 1], "mapped parameter and absent indices");
  Object.prototype[1] = "op";
  try {
    assert.compareArray(mapped(1), ["changed", "op", undefined, 1], "absent index sees Object.prototype");
  } finally {
    delete Object.prototype[1];
  }
  function unmapped(a) {
    "use strict";
    a = "changed";
    return arguments[0];
  }
  assert.sameValue(unmapped("orig"), "orig", "unmapped");
  function viaDefine(a) {
    Object.defineProperty(arguments, "0", { value: "def" });
    return [a, arguments[0]];
  }
  assert.compareArray(viaDefine("x"), ["def", "def"], "mapped value through defineProperty");
})();

// Own-miss lookups on ordinary objects walk the chain with data and accessor holders.
(function () {
  var log = [];
  var base = {
    data: "base-data",
    get acc() { log.push(this); return "base-acc"; },
    get noGetter() { return 1; },
  };
  Object.defineProperty(base, "noGetter", { get: undefined, configurable: true });
  var mid = Object.create(base, { midData: { value: "mid-data" } });
  var leaf = Object.create(mid);
  leaf.own = "own";
  assert.sameValue(leaf.own, "own", "own data");
  assert.sameValue(leaf.midData, "mid-data", "one miss");
  assert.sameValue(leaf.data, "base-data", "two misses");
  assert.sameValue(leaf.acc, "base-acc", "inherited getter");
  assert.sameValue(log[0], leaf, "receiver is the original object");
  assert.sameValue(leaf.noGetter, undefined, "accessor without getter");
  assert.sameValue(leaf.absent, undefined, "absent everywhere");
  assert.sameValue(leaf.hasOwnProperty("data"), false, "not own");
  leaf.data = "shadow";
  assert.sameValue(leaf.data, "shadow", "own shadows base");
  assert.sameValue(mid.data, "base-data", "base unchanged");
  assert.sameValue(leaf.length, undefined, "plain object length");

  var arr = [];
  arr.push = "own-push";
  assert.sameValue(arr.push, "own-push", "own shadows method");
  assert.sameValue([].push, Array.prototype.push, "method on the prototype");
  assert.sameValue([].hasOwnProperty, Object.prototype.hasOwnProperty, "method on Object.prototype");
  assert.sameValue([].length, 0, "array length is own");
})();

// Library callers share the same read path.
(function () {
  setIndex(Array.prototype, 1, "m");
  try {
    assert.sameValue(new Array(3).join("-"), "-m-", "join sees the inherited index");
    assert.compareArray([...new Array(3)], [undefined, "m", undefined], "iteration sees the inherited index");
    assert.compareArray(Array.from(new Array(2)), [undefined, "m"], "Array.from");
  } finally {
    delete Array.prototype[1];
  }
  assert.sameValue(new Array(3).join("-"), "--", "join without the inherited index");
  assert.sameValue(new Array(200).join("ab").length, 398, "long join of holes");
})();

// A prototype link that is not an empty Array, or a very deep chain, still answers.
(function () {
  var a = new Array(3);
  Object.setPrototypeOf(a, [10, 20, 30, 40]);
  assert.sameValue(a[1], 20, "non-empty Array prototype supplies the hole");
  assert.sameValue(a[3], 40, "non-empty Array prototype supplies an out-of-range index");
  assert.sameValue(a[4], undefined, "absent from the whole chain");

  var b = new Array(3);
  Object.setPrototypeOf(b, new Uint8Array([7, 8, 9, 10]));
  assert.sameValue(b[1], 8, "TypedArray prototype supplies the hole");
  assert.sameValue(b[3], 10, "TypedArray prototype supplies an out-of-range index");

  var c = new Array(3);
  Object.setPrototypeOf(c, function () {});
  assert.sameValue(c[1], undefined, "function prototype without the index");
  Object.getPrototypeOf(c)[1] = "fn";
  assert.sameValue(c[1], "fn", "function prototype with the index");

  var deep = { 2: "far" };
  for (var i = 0; i < 70; i++) deep = Object.create(deep);
  var d = new Array(4);
  Object.setPrototypeOf(d, deep);
  assert.sameValue(d[2], "far", "index at the end of a 70-link chain");
  assert.sameValue(d[1], undefined, "absent from a 70-link chain");
})();

// Digit-leading keys that are not array indices are not indices.
(function () {
  Object.prototype["2x"] = "named";
  Array.prototype["3d"] = "named";
  try {
    var a = new Array(4);
    assert.sameValue(a[2], undefined, "2x is not index 2");
    assert.sameValue(a[3], undefined, "3d is not index 3");
    assert.sameValue(a["2x"], "named", "the named key is still readable");
    var b = [];
    b["1st"] = "own";
    assert.sameValue(b[1], undefined, "own 1st is not index 1");
    assert.sameValue(b["1st"], "own", "own named key");
  } finally {
    delete Object.prototype["2x"];
    delete Array.prototype["3d"];
  }
})();
