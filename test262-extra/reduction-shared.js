/*---
description: >
  Array and TypedArray reductions preserve accumulator selection, traversal,
  callback operands, mutation, and abrupt completion in both directions.
esid: sec-array.prototype.reduce
info: |
  Array.prototype.reduce and reduceRight use HasProperty before Get and skip
  holes when choosing the first accumulator and invoking the callback.
  TypedArray reductions read every index in the initially captured range.
  Calls supply undefined as this and accumulator, value, index, and receiver.
includes: [compareArray.js, detachArrayBuffer.js]
features: [TypedArray, Proxy, resizable-arraybuffer]
---*/

[false, true].forEach(function (typed) {
  ["reduce", "reduceRight"].forEach(function (name) {
    var right = name === "reduceRight";
    var method = typed ? Uint8Array.prototype[name] : Array.prototype[name];
    var receiver = typed ? new Uint8Array([1, 2, 3]) : [1, 2, 3];
    var order = right ? [2, 1, 0] : [0, 1, 2];
    var seen = [];
    var result = method.call(receiver, function (acc, value, index, object) {
      "use strict";
      assert.sameValue(this, undefined, name + " callback this");
      assert.sameValue(arguments.length, 4, name + " argument count");
      assert.sameValue(object, receiver, name + " callback receiver");
      assert.sameValue(value, index + 1, name + " value");
      seen.push(index);
      return acc * 10 + value;
    }, 9);
    assert.sameValue(result, right ? 9321 : 9123, name + " supplied seed");
    assert.compareArray(seen, order, name + " direction");

    seen = [];
    result = method.call(receiver, function (acc, value, index) {
      seen.push(index);
      return acc * 10 + value;
    });
    assert.sameValue(result, right ? 321 : 123, name + " implicit seed");
    assert.compareArray(seen, [1, right ? 0 : 2], name + " skips seed callback");

    var empty = typed ? new Uint8Array(0) : [];
    function never() { throw new Test262Error("callback must not run"); }
    assert.throws(TypeError, function () { method.call(empty, {}); });
    assert.throws(TypeError, function () { method.call(empty, never); });
    assert.sameValue(method.call(empty, never, undefined), undefined);
    var seed = {};
    assert.sameValue(method.call(empty, never, seed), seed, "initial value identity");

    var single = typed ? new Uint8Array([7]) : [7];
    assert.sameValue(method.call(single, never), 7, "singleton needs no call");
    method.call(single, function (acc) {
      assert.sameValue(acc, undefined, "explicit undefined reaches callback");
    }, undefined);

    var sentinel = {};
    var thrown;
    try {
      method.call(receiver, function () { throw sentinel; }, 0);
    } catch (e) { thrown = e; }
    assert.sameValue(thrown, sentinel, name + " callback throw identity");

    var callable = new Proxy(function (acc, value) { return acc + value; }, {});
    assert.sameValue(method.call(receiver, callable, 0), 6, "callable proxy");
    var descriptor = Object.getOwnPropertyDescriptor(
      typed ? Object.getPrototypeOf(Uint8Array.prototype) : Array.prototype, name
    );
    assert.sameValue(descriptor.value.length, 1);
    assert.sameValue(descriptor.value.name, name);
    assert.sameValue(descriptor.writable, true);
    assert.sameValue(descriptor.enumerable, false);
    assert.sameValue(descriptor.configurable, true);
  });
});

["reduce", "reduceRight"].forEach(function (name) {
  var right = name === "reduceRight";
  var method = Array.prototype[name];
  var proto = {1: 7};
  var receiver = Object.create(proto);
  receiver.length = 4;
  receiver[3] = 2;
  var seen = [];
  assert.sameValue(method.call(receiver, function (acc, value, index) {
    seen.push(index);
    return acc * 10 + value;
  }), right ? 27 : 72, "inherited seed and sparse traversal");
  assert.compareArray(seen, [right ? 1 : 3], "absent indices skipped");
  assert.throws(TypeError, function () { method.call(new Array(3), function () {}); });

  var events = [];
  var target = {length: 3, 1: 4};
  var proxy = new Proxy(target, {
    get: function (object, key) { events.push("get:" + key); return object[key]; },
    has: function (object, key) { events.push("has:" + key); return key in object; }
  });
  assert.sameValue(method.call(proxy, function () {
    events.push("callback"); return 8;
  }, 0), 8);
  assert.compareArray(events, right ?
    ["get:length", "has:2", "has:1", "get:1", "callback", "has:0"] :
    ["get:length", "has:0", "has:1", "get:1", "callback", "has:2"]);

  var marker = {};
  var thrown;
  var bad = {get length() { throw marker; }};
  try { method.call(bad, {}); } catch (e) { thrown = e; }
  assert.sameValue(thrown, marker, "length access precedes callback validation");
  ["get", "has"].forEach(function (trap) {
    var handler = {};
    handler[trap] = function (object, key) {
      if (key === "length") return 1;
      throw marker;
    };
    thrown = undefined;
    try { method.call(new Proxy({length: 1, 0: 1}, handler), function () {}, 0); }
    catch (e) { thrown = e; }
    assert.sameValue(thrown, marker, trap + " abrupt propagation");
  });

  receiver = [1, 2, 3];
  seen = [];
  var result = method.call(receiver, function (acc, value, index) {
    seen.push([index, value].join(":"));
    if (seen.length === 1) {
      receiver.length = 1;
      receiver[1] = 8;
      receiver[3] = 9;
    }
    return acc + value;
  }, 0);
  assert.sameValue(result, right ? 12 : 9, "live reads and captured length");
  assert.compareArray(seen, right ? ["2:3", "1:8", "0:1"] : ["0:1", "1:8"]);

  assert.sameValue(method.call("abc", function (acc, value) { return acc + value; }, ""),
    right ? "cba" : "abc", "primitive receiver boxing");
  assert.throws(TypeError, function () { method.call(null, function () {}, 0); });
  assert.throws(TypeError, function () { method.call(undefined, function () {}, 0); });

  var typedMethod = Uint8Array.prototype[name];
  assert.throws(TypeError, function () { typedMethod.call([], function () {}, 0); });
  var ta = new Uint8Array([1, 2, 3]);
  var detached = ta.buffer;
  $DETACHBUFFER(detached);
  assert.throws(TypeError, function () { typedMethod.call(ta, function () {}, 0); });

  ta = new Uint8Array([1, 2, 3]);
  seen = [];
  typedMethod.call(ta, function (acc, value, index) {
    seen.push(value);
    if (seen.length === 1) $DETACHBUFFER(ta.buffer);
    return acc;
  }, 0);
  assert.compareArray(seen, [right ? 3 : 1, undefined, undefined],
    "detachment does not shorten captured range");

  var buffer = new ArrayBuffer(3, {maxByteLength: 5});
  ta = new Uint8Array(buffer);
  ta.set([1, 2, 3]);
  seen = [];
  typedMethod.call(ta, function (acc, value, index) {
    seen.push(value);
    if (seen.length === 1) {
      buffer.resize(5);
      ta[1] = 8;
      ta[3] = 9;
    }
    return acc;
  }, 0);
  assert.compareArray(seen, right ? [3, 8, 1] : [1, 8, 3],
    "growth retains range and live element reads");

  buffer = new ArrayBuffer(3, {maxByteLength: 5});
  ta = new Uint8Array(buffer);
  ta.set([1, 2, 3]);
  seen = [];
  typedMethod.call(ta, function (acc, value) {
    seen.push(value);
    if (seen.length === 1) buffer.resize(1);
    return acc;
  }, 0);
  assert.compareArray(seen, right ? [3, undefined, 1] : [1, undefined, undefined],
    "shrink retains range and live bounds checks");
});

// The accumulator exists only in the reduction while the next accessor runs.
["reduce", "reduceRight"].forEach(function (name) {
  [false, true].forEach(function (implicitSeed) {
    var right = name === "reduceRight";
    var receiver = {length: 2};
    var first = right ? 1 : 0;
    var second = right ? 0 : 1;
    Object.defineProperty(receiver, first, {
      get: function () { return implicitSeed ? {marker: 73} : 1; }
    });
    Object.defineProperty(receiver, second, {
      get: function () {
        $262.gc();
        for (var i = 0; i < 32; i++) ({overwrite: i});
        return 2;
      }
    });
    function callback(acc, value, index) {
      if (index === second) {
        assert.sameValue(acc.marker, 73, name + " accumulator survives next Get");
      }
      return {marker: 73};
    }
    if (implicitSeed) Array.prototype[name].call(receiver, callback);
    else Array.prototype[name].call(receiver, callback, null);
  });
});
