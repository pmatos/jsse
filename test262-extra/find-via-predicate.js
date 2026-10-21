/*---
description: >
  Array and TypedArray find methods share predicate traversal, callback arguments,
  early exit, captured length, abrupt completion and value retention semantics.
esid: sec-findviapredicate
info: |
  FindViaPredicate validates the predicate, visits the captured index range in
  the requested direction, performs Get without HasProperty, and calls the
  predicate with value, index and receiver. A successful result retains the
  value read before the callback; a miss contains index -1 and undefined.
includes: [compareArray.js, detachArrayBuffer.js]
features: [TypedArray, Proxy, array-find-from-last, resizable-arraybuffer]
---*/

var names = ["find", "findIndex", "findLast", "findLastIndex"];

function backwards(name) {
  return name === "findLast" || name === "findLastIndex";
}

function returnsIndex(name) {
  return name === "findIndex" || name === "findLastIndex";
}

function indices(name) {
  return backwards(name) ? [2, 1, 0] : [0, 1, 2];
}

function checkResult(name, result, index, value) {
  assert.sameValue(result, returnsIndex(name) ? index : value, name + " result");
}

[false, true].forEach(function (typed) {
  names.forEach(function (name) {
    var method = typed ? Uint8Array.prototype[name] : Array.prototype[name];
    var receiver = typed ? new Uint8Array([10, 20, 30]) : [10, 20, 30];
    var context = {};
    var seen = [];
    var order = indices(name);

    var result = method.call(receiver, function (value, index, object) {
      "use strict";
      assert.sameValue(this, context, name + " thisArg");
      assert.sameValue(arguments.length, 3, name + " callback argument count");
      assert.sameValue(object, receiver, name + " callback receiver");
      assert.sameValue(value, receiver[index], name + " callback value");
      seen.push(index);
      return index === 1 ? {} : 0;
    }, context);
    checkResult(name, result, 1, 20);
    assert.compareArray(seen, [order[0], 1], name + " exits at first truthy result");

    seen = [];
    result = method.call(receiver, function (value, index) {
      seen.push(index);
      return false;
    });
    checkResult(name, result, -1, undefined);
    assert.compareArray(seen, order, name + " miss visits full range");

    var empty = typed ? new Uint8Array(0) : [];
    assert.throws(TypeError, function () {
      method.call(empty, {});
    }, name + " validates predicate even for empty input");

    var sentinel = {};
    seen = [];
    try {
      method.call(receiver, function (value, index) {
        seen.push(index);
        throw sentinel;
      });
      throw new Test262Error("callback throw must propagate");
    } catch (error) {
      assert.sameValue(error, sentinel, name + " propagates exact callback throw");
    }
    assert.compareArray(seen, [order[0]], name + " stops after callback throw");

    // Changes to an unvisited element are read at its visitation time.
    seen = [];
    method.call(receiver, function (value, index) {
      seen.push(value);
      if (seen.length === 1) receiver[1] = 99;
      return false;
    });
    assert.compareArray(seen, [backwards(name) ? 30 : 10, 99,
      backwards(name) ? 10 : 30], name + " reads later mutations");

    var predicate = new Proxy(function (value, index, object) {
      assert.sameValue(object, receiver, name + " proxy predicate receiver");
      return index === 1;
    }, {});
    checkResult(name, method.call(receiver, predicate), 1, 99);
  });
});

names.forEach(function (name) {
  var method = Array.prototype[name];
  var order = indices(name);
  var log = [];
  var prototype = Object.create(Array.prototype);
  prototype[1] = 41;
  var sparse = new Array(3);
  Object.setPrototypeOf(sparse, prototype);
  var receiver = new Proxy(sparse, {
    get: function (target, key, object) {
      log.push("get:" + key);
      return Reflect.get(target, key, object);
    },
    has: function (target, key) {
      log.push("has:" + key);
      return Reflect.has(target, key);
    }
  });
  var values = [];
  checkResult(name, method.call(receiver, function (value, index, object) {
    assert.sameValue(object, receiver, name + " preserves proxy receiver");
    values.push(value);
    return false;
  }), -1, undefined);
  assert.compareArray(log, ["get:length", "get:" + order[0], "get:1",
    "get:" + order[2]], name + " reads holes without HasProperty");
  assert.compareArray(values, [undefined, 41, undefined], name + " inherited value and holes");

  receiver = [1, 2, 3];
  var visited = [];
  values = [];
  method.call(receiver, function (value, index) {
    visited.push(index);
    values.push(value);
    if (visited.length === 1) {
      receiver[1] = 99;
      delete receiver[order[2]];
      receiver[3] = 4;
    }
    return false;
  });
  assert.compareArray(visited, order, name + " captures initial length");
  assert.compareArray(values, [backwards(name) ? 3 : 1, 99, undefined],
    name + " deleted elements remain in captured range");

  // ToObject and LengthOfArrayLike precede predicate validation; no index is
  // read after invalid predicate validation, including a zero captured length.
  [0, 1].forEach(function (length) {
    log = [];
    var object = {
      get length() {
        log.push("length");
        return { valueOf: function () { log.push("coerce"); return length; } };
      },
      get 0() { log.push("index"); return 1; }
    };
    assert.throws(TypeError, function () { method.call(object, null); });
    assert.compareArray(log, ["length", "coerce"], name + " validation order");
  });
  assert.throws(TypeError, function () { method.call(null, function () {}); });

  var sentinel = {};
  var calls = 0;
  var object = { length: 3 };
  Object.defineProperty(object, String(order[0]), {
    get: function () { throw sentinel; }
  });
  try {
    method.call(object, function () { calls++; return true; });
    throw new Test262Error("getter throw must propagate");
  } catch (error) {
    assert.sameValue(error, sentinel, name + " propagates exact getter throw");
  }
  assert.sameValue(calls, 0, name + " does not call predicate after getter throw");

  object = { get length() { throw sentinel; } };
  try {
    method.call(object, null);
    throw new Test262Error("length getter throw must precede predicate validation");
  } catch (error) {
    assert.sameValue(error, sentinel, name + " length throw precedes validation");
  }

  // The successful value is the value read before the callback, even after its
  // source property is deleted and the callback explicitly runs collection.
  receiver = [{ marker: 123 }];
  var result = method.call(receiver, function () {
    "use strict";
    delete receiver[0];
    $262.gc();
    return true;
  });
  if (returnsIndex(name)) {
    assert.sameValue(result, 0);
  } else {
    assert.sameValue(result.marker, 123, name + " returns retained pre-callback value");
  }
});

names.forEach(function (name) {
  var method = Uint8Array.prototype[name];
  var order = indices(name);
  ["detach", "shrink", "grow"].forEach(function (change) {
    var buffer = new ArrayBuffer(3, { maxByteLength: 6 });
    var receiver = new Uint8Array(buffer);
    receiver.set([1, 2, 3]);
    var seen = [];
    var visited = [];
    var result = method.call(receiver, function (value, index) {
      seen.push(value);
      visited.push(index);
      if (seen.length === 1) {
        if (change === "detach") $DETACHBUFFER(buffer);
        else buffer.resize(change === "shrink" ? 0 : 6);
      }
      return false;
    });
    checkResult(name, result, -1, undefined);
    assert.compareArray(visited, order, name + " " + change + " preserves captured length");
    var expected = change === "grow" ? (backwards(name) ? [3, 2, 1] : [1, 2, 3]) :
      [backwards(name) ? 3 : 1, undefined, undefined];
    assert.compareArray(seen, expected, name + " " + change + " reads current buffer state");
  });

  var buffer = new ArrayBuffer(3);
  var detached = new Uint8Array(buffer);
  $DETACHBUFFER(buffer);
  var calls = 0;
  assert.throws(TypeError, function () {
    method.call(detached, function () { calls++; return true; });
  }, name + " validates initially detached view");
  assert.sameValue(calls, 0);
  assert.throws(TypeError, function () { method.call([], function () {}); },
    name + " rejects non-TypedArray receiver");
});
