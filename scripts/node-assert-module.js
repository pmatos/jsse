// CommonJS `assert` selector for bundled library dependencies.
//
// css-tree's own test suite imports Node's built-in `assert` and uses only
// strictEqual, notStrictEqual, deepStrictEqual, throws, and doesNotThrow.
// Node keeps its native module (independent oracle); JSSE gets a minimal
// same-surface implementation below.

if (typeof __host_write !== "undefined") {
  function isObjectLike(value) {
    return value !== null && typeof value === "object";
  }

  function sameValue(a, b) {
    return Object.is(a, b);
  }

  function looseDeepEqual(a, b, seen) {
    if (a === b) return true;
    // Node's legacy (non-strict) deepEqual treats two NaNs as equal, like
    // strict deepEqual, but otherwise compares primitives with `==`.
    if (!isObjectLike(a) || !isObjectLike(b)) {
      if (typeof a === "number" && typeof b === "number" && Number.isNaN(a) && Number.isNaN(b)) {
        return true;
      }
      return a == b; // eslint-disable-line eqeqeq
    }

    var isArrayA = Array.isArray(a);
    var isArrayB = Array.isArray(b);
    if (isArrayA !== isArrayB) return false;
    if (isArrayA && a.length !== b.length) return false;

    var seenPair = seen.get(a);
    if (seenPair && seenPair.has(b)) return true;
    if (!seenPair) {
      seenPair = new Set();
      seen.set(a, seenPair);
    }
    seenPair.add(b);

    var keysA = Object.keys(a);
    var keysB = Object.keys(b);
    if (keysA.length !== keysB.length) return false;

    for (var i = 0; i < keysA.length; i++) {
      var key = keysA[i];
      if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
      if (!looseDeepEqual(a[key], b[key], seen)) return false;
    }

    return true;
  }

  function deepEqual(a, b, seen) {
    if (sameValue(a, b)) return true;
    if (!isObjectLike(a) || !isObjectLike(b)) return false;
    if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;

    var isArrayA = Array.isArray(a);
    var isArrayB = Array.isArray(b);
    if (isArrayA !== isArrayB) return false;
    if (isArrayA && a.length !== b.length) return false;

    var seenPair = seen.get(a);
    if (seenPair && seenPair.has(b)) return true;
    if (!seenPair) {
      seenPair = new Set();
      seen.set(a, seenPair);
    }
    seenPair.add(b);

    var keysA = Object.keys(a).concat(Object.getOwnPropertySymbols(a));
    var keysB = Object.keys(b).concat(Object.getOwnPropertySymbols(b));
    if (keysA.length !== keysB.length) return false;

    for (var i = 0; i < keysA.length; i++) {
      var key = keysA[i];
      if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
      if (!deepEqual(a[key], b[key], seen)) return false;
    }

    return true;
  }

  function AssertionError(message) {
    var error = new Error(message);
    error.name = "AssertionError";
    return error;
  }

  function fail(message) {
    throw AssertionError(message);
  }

  function assert(value, message) {
    if (!value) fail(message || "The expression evaluated to a falsy value");
  }

  assert.ok = assert;

  assert.strictEqual = function (actual, expected, message) {
    if (!sameValue(actual, expected)) {
      fail(message || actual + " !== " + expected);
    }
  };

  assert.notStrictEqual = function (actual, expected, message) {
    if (sameValue(actual, expected)) {
      fail(message || actual + " === " + expected);
    }
  };

  assert.deepStrictEqual = function (actual, expected, message) {
    if (!deepEqual(actual, expected, new Map())) {
      fail(
        message ||
          "Expected values to be strictly deep-equal:\n" +
          JSON.stringify(actual) +
          "\nvs\n" +
          JSON.stringify(expected)
      );
    }
  };

  assert.notDeepStrictEqual = function (actual, expected, message) {
    if (deepEqual(actual, expected, new Map())) {
      fail(message || "Expected values not to be strictly deep-equal");
    }
  };

  assert.deepEqual = function (actual, expected, message) {
    if (!looseDeepEqual(actual, expected, new Map())) {
      fail(
        message ||
          "Expected values to be deep-equal:\n" +
          JSON.stringify(actual) +
          "\nvs\n" +
          JSON.stringify(expected)
      );
    }
  };

  assert.notDeepEqual = function (actual, expected, message) {
    if (looseDeepEqual(actual, expected, new Map())) {
      fail(message || "Expected values not to be deep-equal");
    }
  };

  function matchesExpected(error, expected) {
    if (expected === undefined) return true;
    if (expected instanceof RegExp) return expected.test(String(error));
    if (typeof expected === "function") {
      if (expected.prototype instanceof Error || expected === Error) {
        return error instanceof expected;
      }
      return Boolean(expected(error));
    }
    return true;
  }

  assert.throws = function (fn, expected, message) {
    var threw = false;
    var thrown;
    try {
      fn();
    } catch (error) {
      threw = true;
      thrown = error;
    }
    if (!threw) {
      fail(
        (typeof expected === "string" ? expected : message) ||
          "Missing expected exception"
      );
    }
    var expectedIsMessage = typeof expected === "string" && message === undefined;
    if (!expectedIsMessage && !matchesExpected(thrown, expected)) {
      throw thrown;
    }
  };

  assert.doesNotThrow = function (fn, message) {
    try {
      fn();
    } catch (error) {
      fail(
        message ||
          "Got unwanted exception: " + (error && error.message ? error.message : error)
      );
    }
  };

  assert.fail = function (message) {
    fail(message);
  };

  module.exports = assert;
} else {
  module.exports = require("node:assert");
}
