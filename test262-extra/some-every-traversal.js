/*---
description: Array and TypedArray predicate quantification preserves reads, captured ranges, callback operands, and short-circuiting
esid: sec-array.prototype.some
info: |
  Array.prototype.some/every use HasProperty before Get and call the predicate
  over the initially captured range. %TypedArray%.prototype.some/every validate
  once, then Get each index in that range, including after resize/detachment.
includes: [compareArray.js]
features: [Proxy, TypedArray, BigInt, resizable-arraybuffer]
---*/

for (var name of ["some", "every"]) {
  var stop = name === "some";
  var method = Array.prototype[name];
  var log = [];
  var context = {};
  var target = Object.create({ 1: "inherited" });
  target.length = 4;
  target[2] = "own";
  Object.defineProperty(target, "3", {
    get: function () { throw new Test262Error("past short-circuit"); }
  });
  var receiver = new Proxy(target, {
    has: function (o, k) { log.push("has " + k); return Reflect.has(o, k); },
    get: function (o, k, r) { log.push("get " + k); return Reflect.get(o, k, r); }
  });
  var result = method.call(receiver, function (value, index, object) {
    "use strict";
    assert.sameValue(this, context);
    assert.sameValue(arguments.length, 3);
    assert.sameValue(object, receiver);
    assert.sameValue(value, index === 1 ? "inherited" : "own");
    log.push("call " + index);
    return index === 2 ? stop : !stop;
  }, context);
  assert.sameValue(result, stop);
  assert.compareArray(log, ["get length", "has 0", "has 1", "get 1", "call 1",
    "has 2", "get 2", "call 2"]);

  // An empty range still performs callable validation after length access.
  log = [];
  assert.throws(TypeError, function () {
    method.call({ get length() { log.push("length"); return 0; } }, null);
  });
  assert.compareArray(log, ["length"]);
  var sentinel = {};
  try {
    method.call({ get length() { throw sentinel; } }, null);
    throw new Test262Error("length must throw");
  } catch (error) { assert.sameValue(error, sentinel); }
  assert.throws(TypeError, function () { method.call(null, function () {}); });
  assert.sameValue(method.call([], function () {
    throw new Test262Error("empty input");
  }), !stop);

  // Mutation affects presence/values but cannot extend the captured range.
  var array = [10, 20, 30];
  var seen = [];
  assert.sameValue(method.call(array, function (value, index, object) {
    "use strict";
    assert.sameValue(this, undefined);
    assert.sameValue(object, array);
    seen.push(index + ":" + value);
    if (index === 0) {
      delete array[1];
      array[2] = 33;
      array.push(44);
    }
    return !stop;
  }), !stop);
  assert.compareArray(seen, ["0:10", "2:33"]);

  // Generic use boxes primitive receivers and ToBoolean does not coerce objects.
  assert.sameValue(method.call("ab", function (value, index, object) {
    assert.sameValue(typeof object, "object");
    assert.sameValue(value, "ab"[index]);
    return !stop;
  }), !stop);
  var truthy = { valueOf: function () { throw new Test262Error("ToBoolean"); } };
  assert.sameValue(method.call([1], function () { return truthy; }), true);
  try {
    method.call([1], function () { throw sentinel; });
    throw new Test262Error("callback must throw");
  } catch (error) { assert.sameValue(error, sentinel); }

  for (var Constructor of [Uint8Array, BigInt64Array]) {
    var values = Constructor === Uint8Array ? [1, 2, 3] : [1n, 2n, 3n];
    var typed = new Constructor(values);
    seen = [];
    assert.sameValue(typed[name](function (value, index, object) {
      "use strict";
      assert.sameValue(this, context);
      assert.sameValue(arguments.length, 3);
      assert.sameValue(object, typed);
      assert.sameValue(value, values[index]);
      seen.push(index);
      return index === 1 ? stop : !stop;
    }, context), stop);
    assert.compareArray(seen, [0, 1]);
    assert.sameValue(new Constructor(0)[name](function () {
      throw new Test262Error("empty typed input");
    }), !stop);
    assert.throws(TypeError, function () { new Constructor(0)[name](null); });
    assert.throws(TypeError, function () {
      Constructor.prototype[name].call([], function () {});
    });
    try {
      typed[name](function () { throw sentinel; });
      throw new Test262Error("typed callback must throw");
    } catch (error) { assert.sameValue(error, sentinel); }
  }

  // All captured indices are visited even after the backing store changes.
  for (var action of ["shrink", "grow", "detach"]) {
    var buffer = new ArrayBuffer(3, { maxByteLength: 6 });
    typed = new Uint8Array(buffer);
    typed.set([1, 2, 3]);
    seen = [];
    assert.sameValue(typed[name](function (value, index) {
      seen.push(value);
      if (index === 0) {
        if (action === "shrink") buffer.resize(1);
        if (action === "grow") buffer.resize(6);
        if (action === "detach") $262.detachArrayBuffer(buffer);
      }
      return !stop;
    }), !stop);
    assert.compareArray(seen, action === "grow" ? [1, 2, 3] : [1, undefined, undefined]);
  }
  buffer = new ArrayBuffer(1);
  typed = new Uint8Array(buffer);
  $262.detachArrayBuffer(buffer);
  assert.throws(TypeError, function () { typed[name](function () {}); });

  buffer = new ArrayBuffer(3, { maxByteLength: 6 });
  typed = new Uint8Array(buffer, 0, 3);
  typed.set([1, 2, 3]);
  seen = [];
  assert.sameValue(typed[name](function (value, index) {
    seen.push(value);
    if (index === 0) buffer.resize(1);
    return !stop;
  }), !stop);
  assert.compareArray(seen, [1, undefined, undefined]);
  assert.throws(TypeError, function () { typed[name](function () {}); });

  var calls = 0;
  var predicate = new Proxy(function () {}, {
    apply: function (fn, thisArg, args) {
      calls++;
      assert.sameValue(thisArg, context);
      assert.sameValue(args.length, 3);
      return stop;
    }
  });
  for (var input of [[1, 2], new Uint8Array([1, 2])]) {
    calls = 0;
    assert.sameValue(input[name](predicate, context), stop);
    assert.sameValue(calls, 1);
  }
  var revocable = Proxy.revocable(function () {}, {});
  revocable.revoke();
  assert.sameValue([][name](revocable.proxy), !stop);
  assert.sameValue(new Uint8Array(0)[name](revocable.proxy), !stop);
}
