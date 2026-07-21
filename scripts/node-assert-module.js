// CommonJS `assert` selector for bundled library dependencies.
//
// Node's `assert` core module has no JS-only equivalent installed by
// node-shim.js (unlike Buffer/util), so this selector supplies one: on real
// Node the native module runs as the reference oracle; on JSSE a compact
// polyfill covers the common assert.* surface so mocha/tap-style suites that
// assert with `require('assert')` (rather than a bundled matcher library)
// still execute.

if (typeof __host_write !== "undefined") {
  function AssertionError(options) {
    options = options || {};
    Error.call(this, options.message || "");
    this.name = "AssertionError";
    this.message = options.message || "";
    this.actual = options.actual;
    this.expected = options.expected;
    this.operator = options.operator;
    this.code = "ERR_ASSERTION";
    if (Error.captureStackTrace) Error.captureStackTrace(this, AssertionError);
  }
  AssertionError.prototype = Object.create(Error.prototype);
  AssertionError.prototype.constructor = AssertionError;

  function fail(actual, expected, message, operator) {
    var hasArgs = arguments.length > 0;
    var msg = hasArgs && arguments.length < 3 ? actual : message;
    throw new AssertionError({
      message: msg || (operator ? operator : "Failed"),
      actual: hasArgs && arguments.length >= 3 ? actual : undefined,
      expected: hasArgs && arguments.length >= 3 ? expected : undefined,
      operator: operator || "fail",
    });
  }

  function isRegExp(v) {
    return Object.prototype.toString.call(v) === "[object RegExp]";
  }
  function isDate(v) {
    return Object.prototype.toString.call(v) === "[object Date]";
  }

  // Recursive structural equality. `strict` selects Object.is()/=== for
  // primitives and requires matching constructors/key sets; non-strict
  // (legacy) mode uses `==` and only compares own enumerable keys.
  function deepEqual(a, b, strict, seen) {
    if (strict ? Object.is(a, b) : a == b) return true;
    if (a === null || b === null || typeof a !== "object" || typeof b !== "object") {
      return false;
    }
    if (strict && Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
    if (isRegExp(a) || isRegExp(b)) {
      return isRegExp(a) && isRegExp(b) && a.source === b.source && a.flags === b.flags;
    }
    if (isDate(a) || isDate(b)) {
      return isDate(a) && isDate(b) && a.getTime() === b.getTime();
    }
    seen = seen || [];
    for (var i = 0; i < seen.length; i++) {
      if (seen[i][0] === a && seen[i][1] === b) return true;
    }
    seen.push([a, b]);

    var aIsArray = Array.isArray(a), bIsArray = Array.isArray(b);
    if (aIsArray !== bIsArray) return false;
    if (aIsArray) {
      if (a.length !== b.length) return false;
      for (var i2 = 0; i2 < a.length; i2++) {
        if (!deepEqual(a[i2], b[i2], strict, seen)) return false;
      }
      return true;
    }

    var aKeys = Object.keys(a), bKeys = Object.keys(b);
    if (strict && aKeys.length !== bKeys.length) return false;
    for (var k = 0; k < aKeys.length; k++) {
      var key = aKeys[k];
      if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
      if (!deepEqual(a[key], b[key], strict, seen)) return false;
    }
    if (!strict) return true;
    for (var k2 = 0; k2 < bKeys.length; k2++) {
      if (!Object.prototype.hasOwnProperty.call(a, bKeys[k2])) return false;
    }
    return true;
  }

  function assert(value, message) {
    if (!value) {
      throw new AssertionError({
        message: message || "The expression evaluated to a falsy value",
        actual: value,
        expected: true,
        operator: "==",
      });
    }
  }
  assert.ok = assert;

  assert.equal = function (actual, expected, message) {
    if (!(actual == expected)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "==" });
    }
  };
  assert.notEqual = function (actual, expected, message) {
    if (actual == expected) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "!=" });
    }
  };
  assert.strictEqual = function (actual, expected, message) {
    if (!Object.is(actual, expected)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "===" });
    }
  };
  assert.notStrictEqual = function (actual, expected, message) {
    if (Object.is(actual, expected)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "!==" });
    }
  };
  assert.deepEqual = function (actual, expected, message) {
    if (!deepEqual(actual, expected, false)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "deepEqual" });
    }
  };
  assert.notDeepEqual = function (actual, expected, message) {
    if (deepEqual(actual, expected, false)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "notDeepEqual" });
    }
  };
  assert.deepStrictEqual = function (actual, expected, message) {
    if (!deepEqual(actual, expected, true)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "deepStrictEqual" });
    }
  };
  assert.notDeepStrictEqual = function (actual, expected, message) {
    if (deepEqual(actual, expected, true)) {
      throw new AssertionError({ message: message, actual: actual, expected: expected, operator: "notDeepStrictEqual" });
    }
  };

  function matchesError(err, matcher) {
    if (matcher === undefined) return true;
    if (typeof matcher === "function") {
      return err instanceof matcher || matcher(err) === true;
    }
    if (isRegExp(matcher)) return matcher.test(String(err && err.message));
    if (typeof matcher === "object" && matcher !== null) {
      return Object.keys(matcher).every(function (key) {
        return err && deepEqual(err[key], matcher[key], false);
      });
    }
    return false;
  }

  assert.throws = function (fn, matcher, message) {
    var threw = false, thrown;
    try {
      fn();
    } catch (e) {
      threw = true;
      thrown = e;
    }
    if (!threw) {
      throw new AssertionError({ message: message || "Missing expected exception", operator: "throws" });
    }
    if (typeof matcher !== "string" && !matchesError(thrown, matcher)) {
      throw thrown;
    }
  };
  assert.doesNotThrow = function (fn, matcher, message) {
    try {
      fn();
    } catch (e) {
      if (typeof matcher === "string" || matcher === undefined || matchesError(e, matcher)) {
        throw new AssertionError({
          message: (message ? message + ": " : "") + "Got unwanted exception: " + (e && e.message),
          operator: "doesNotThrow",
        });
      }
      throw e;
    }
  };
  assert.ifError = function (value) {
    if (value !== null && value !== undefined) {
      throw value instanceof Error ? value : new AssertionError({
        message: "ifError got unwanted exception: " + value,
        actual: value,
        expected: null,
        operator: "ifError",
      });
    }
  };
  assert.fail = fail;
  assert.AssertionError = AssertionError;

  module.exports = assert;
} else {
  module.exports = require("node:assert");
}
