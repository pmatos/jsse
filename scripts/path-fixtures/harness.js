// Runs a verbatim Node test file (test/parallel/test-path-*.js, Node v26.8.2)
// unchanged on jsse --node and on Node: the runner prepends this file, wraps
// the test in `(function (require, __filename, __dirname) { ... })(...)` and
// appends `console.log("ok")`. Only the pieces of `assert` and
// `test/common` that the path tests use are provided.
function __nodePathLoad(id) {
  return typeof __node_builtin === "function" ? __node_builtin(id) : require(id);
}
var __nodePathInspect = typeof util === "object" && util !== null
  ? util.inspect
  : require("util").inspect;
var __nodePathAssert = (function () {
  function assert(value, message) {
    if (!value) throw new Error(message === undefined ? "assertion failed" : String(message));
  }
  assert.ok = assert;
  function show(value) {
    return typeof value === "string" ? JSON.stringify(value) : String(value);
  }
  function fail(message, fallback) {
    throw new Error(message === undefined ? fallback : String(message));
  }
  function deepEqual(a, b) {
    if (Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
    if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
    var keys = Object.keys(a);
    if (keys.length !== Object.keys(b).length) return false;
    return keys.every(function (key) { return deepEqual(a[key], b[key]); });
  }
  function matches(actual, expected, key) {
    if (expected instanceof RegExp) return expected.test(String(actual));
    return Object.is(actual, expected);
  }
  var methods = {
    strictEqual: function (actual, expected, message) {
      if (!Object.is(actual, expected)) {
        fail(message, "expected " + show(expected) + ", got " + show(actual));
      }
    },
    deepStrictEqual: function (actual, expected, message) {
      if (!deepEqual(actual, expected)) {
        fail(message, "expected " + JSON.stringify(expected) + ", got " + JSON.stringify(actual));
      }
    },
    throws: function (fn, expected) {
      var threw = false;
      var error;
      try { fn(); } catch (e) { threw = true; error = e; }
      if (!threw) fail(undefined, "missing expected exception");
      if (expected instanceof RegExp) {
        if (!matches(error && error.message, expected)) {
          fail(undefined, "error " + show(error && error.message) + " does not match " + expected);
        }
      } else if (expected !== null && typeof expected === "object") {
        Object.keys(expected).forEach(function (key) {
          if (!matches(error[key], expected[key])) {
            fail(undefined, "error." + key + ": expected " + show(expected[key]) + ", got " + show(error[key]));
          }
        });
      }
    },
  };
  Object.keys(methods).forEach(function (key) { assert[key] = methods[key]; });
  return assert;
})();
var __nodePathCommon = {
  isWindows: false,
  skip: function () {},
  invalidArgTypeHelper: function (input) {
    if (input == null) return " Received " + input;
    if (typeof input === "function") return " Received function " + input.name;
    if (typeof input === "object") {
      if (input.constructor && input.constructor.name) {
        return " Received an instance of " + input.constructor.name;
      }
      return " Received " + __nodePathInspect(input, { depth: -1 });
    }
    var inspected = __nodePathInspect(input, { colors: false });
    if (inspected.length > 28) inspected = inspected.slice(0, 25) + "...";
    return " Received type " + typeof input + " (" + inspected + ")";
  },
};
var __nodePathRequire = function (id) {
  function unavailable() {
    throw new Error("only reached by the Windows-only branches of the tests");
  }
  switch (id) {
    case "../common": return __nodePathCommon;
    case "assert": return __nodePathAssert;
    case "path":
    case "path/posix":
    case "path/win32":
      return __nodePathLoad(id);
    case "../common/fixtures":
    case "child_process":
      return {
        path: unavailable,
        spawnSync: unavailable,
      };
  }
  throw new Error("test requires unsupported module " + id);
};
