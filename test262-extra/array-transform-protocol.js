/*---
description: Array and TypedArray map/filter retain callback, species, and indexed-access ordering
esid: sec-array.prototype.map
includes: [compareArray.js]
features: [Proxy, Symbol.species, TypedArray, resizable-arraybuffer, BigInt]
---*/

for (var name of ["map", "filter"]) {
  var filter = name === "filter";
  var calls = [];
  var source = new Proxy({ 0: 2, 2: 7, length: 3 }, {
    get: function(target, key) {
      calls.push("get:" + key);
      return target[key];
    },
    has: function(target, key) {
      calls.push("has:" + key);
      return key in target;
    }
  });
  var thisArg = { tag: 19 };
  var result = Array.prototype[name].call(source, function(value, index, object) {
    assert.sameValue(this, thisArg);
    assert.sameValue(object, source);
    assert.sameValue(arguments.length, 3);
    calls.push("call:" + index);
    return filter ? -1 : value * 2;
  }, thisArg);
  assert.compareArray(calls, [
    "get:length", "has:0", "get:0", "call:0", "has:1", "has:2", "get:2", "call:2"
  ]);
  assert.sameValue(result.length, filter ? 2 : 3);
  assert.sameValue(result[0], filter ? 2 : 4);
  assert.sameValue(result[filter ? 1 : 2], filter ? 7 : 14);
  if (!filter) assert.sameValue(1 in result, false);

  var inherited = Object.create({ 1: 8 });
  inherited.length = 3;
  assert.sameValue(Array.prototype[name].call(inherited, function(v) { return v; })[filter ? 0 : 1], 8);

  var mutated = [1, 2, 3];
  var visited = [];
  var changed = mutated[name](function(value, index) {
    visited.push(index);
    if (index === 0) {
      mutated.push(4);
      delete mutated[1];
      mutated[2] = 9;
    }
    return value;
  });
  assert.compareArray(visited, [0, 2]);
  assert.sameValue(changed.length, filter ? 2 : 3);
  assert.sameValue(changed[filter ? 1 : 2], 9);

  for (var typed of [false, true]) {
    var trace = [];
    var input = typed ? new Uint8Array([1, 2, 3]) : [1, 2, 3];
    input.constructor = {};
    Object.defineProperty(input.constructor, Symbol.species, {
      get: function() {
        trace.push("species");
        return function(size) {
          trace.push("size:" + size);
          if (typed) return new Uint8Array(size);
          return new Proxy(new Array(size), {
            defineProperty: function(target, key, descriptor) {
              trace.push("define:" + key);
              return Reflect.defineProperty(target, key, descriptor);
            }
          });
        };
      }
    });
    var output = input[name](function(value, index, object) {
      assert.sameValue(object, input);
      assert.sameValue(this, thisArg);
      trace.push("call:" + index);
      return filter ? (index === 1 ? 0 : {}) : value + 10;
    }, thisArg);
    var expected = typed && filter ? ["call:0", "call:1", "call:2", "species", "size:2"] :
      ["species", "size:" + (filter ? 0 : 3), "call:0"];
    if (!(typed && filter)) {
      if (!typed) expected.push("define:0");
      expected.push("call:1");
      if (!typed && !filter) expected.push("define:1");
      expected.push("call:2");
      if (!typed) expected.push("define:" + (filter ? 1 : 2));
    }
    assert.compareArray(trace, expected);
    assert.sameValue(output.length, filter ? 2 : 3);
    assert.sameValue(output[0], filter ? 1 : 11);
    assert.sameValue(output[filter ? 1 : 2], filter ? 3 : 13);

    var empty = typed ? new Uint8Array(0) : [];
    assert.throws(TypeError, function() { empty[name](); });
    assert.sameValue(empty[name](function() { throw new Test262Error("empty callback"); }).length, 0);
    var marker = {};
    var throws = typed ? new Uint8Array([1, 2]) : [1, 2];
    try {
      throws[name](function() { throw marker; });
      throw new Test262Error("missing callback throw");
    } catch (error) {
      assert.sameValue(error, marker);
    }

    var speciesCalls = 0;
    throws.constructor = {};
    Object.defineProperty(throws.constructor, Symbol.species, {
      get: function() { speciesCalls++; throw marker; }
    });
    var callbackCalls = 0;
    try {
      throws[name](function(value) { callbackCalls++; return value; });
      throw new Test262Error("missing species throw");
    } catch (error) {
      assert.sameValue(error, marker);
    }
    assert.sameValue(speciesCalls, 1);
    assert.sameValue(callbackCalls, typed && filter ? 2 : 0);
  }

  var buffer = new ArrayBuffer(3, { maxByteLength: 6 });
  var tracking = new Uint8Array(buffer);
  tracking.set([1, 2, 3]);
  var read = [];
  var resized = tracking[name](function(value, index) {
    read.push(value);
    if (index === 0) buffer.resize(1);
    return filter ? true : (value === undefined ? 9 : value);
  });
  assert.compareArray(read, [1, undefined, undefined]);
  assert.compareArray(Array.from(resized), filter ? [1, 0, 0] : [1, 9, 9]);

  var detached = new Uint8Array([4, 5, 6]);
  var detachedRead = [];
  var detachResult = detached[name](function(value, index) {
    detachedRead.push(value);
    if (index === 0) $262.detachArrayBuffer(detached.buffer);
    return filter ? true : (value === undefined ? 9 : value);
  });
  assert.compareArray(detachedRead, [4, undefined, undefined]);
  assert.compareArray(Array.from(detachResult), filter ? [4, 0, 0] : [4, 9, 9]);

  var big = new BigInt64Array([1n, 2n, 3n]);
  var bigResult = big[name](function(value) { return filter ? value !== 2n : value + 4n; });
  assert.compareArray(Array.from(bigResult), filter ? [1n, 3n] : [5n, 6n, 7n]);

  var proxyCallback = new Proxy(function(value) { return value; }, {
    apply: function(target, receiver, args) { return Reflect.apply(target, receiver, args); }
  });
  assert.sameValue(new Uint8Array([3])[name](proxyCallback)[0], 3);
  assert.sameValue(Array.prototype[name].call("ab", function(value) { return value; }).join(""), "ab");
}

// Fresh values must survive callbacks and observable result definitions. The
// filter result is the original getter value, rather than the predicate result.
for (var gcName of ["map", "filter"]) {
  var gcSource = [0];
  Object.defineProperty(gcSource, "0", {
    get: function() { return { tag: 73 }; }
  });
  gcSource.constructor = {};
  gcSource.constructor[Symbol.species] = function() {
    $262.gc();
    return new Proxy({}, {
      defineProperty: function(target, key, descriptor) {
        $262.gc();
        assert.sameValue(descriptor.value.tag, 73);
        return Reflect.defineProperty(target, key, descriptor);
      }
    });
  };
  var gcOutput = gcSource[gcName](function(value) {
    $262.gc();
    return { tag: value.tag };
  });
  assert.sameValue(gcOutput[0].tag, 73);
}
