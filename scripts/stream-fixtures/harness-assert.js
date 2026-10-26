// Prepended to a verbatim Node test (test/parallel/test-stream-*.js, v26.8.2)
// by scripts/run-node-stream-oracle.sh, ahead of harness.js. It is the subset
// of `assert` the tests use; the same text runs on jsse --node and on Node, so
// both engines answer to the same assertions.
var __sInspect = typeof util === "object" && util !== null && util.inspect
  ? util.inspect
  : require("util").inspect;
var __sAssert = (function () {
  class AssertionError extends Error {
    constructor(message) {
      super(message);
      this.name = "AssertionError";
      this.code = "ERR_ASSERTION";
    }
  }
  function assert(value, message) {
    if (!value) fail(message, "assertion failed");
  }
  assert.ok = assert;
  assert.AssertionError = AssertionError;
  function show(value) {
    try { return __sInspect(value); } catch (e) { return String(value); }
  }
  function fail(message, fallback) {
    if (message instanceof Error) throw message;
    throw new AssertionError(message === undefined ? fallback : String(message));
  }
  function ownKeys(o) {
    return Reflect.ownKeys(o).filter(function (k) {
      return Object.prototype.propertyIsEnumerable.call(o, k);
    });
  }
  function tag(o) { return Object.prototype.toString.call(o); }
  function deepEqual(a, b, loose) {
    if (loose ? a == b : Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
      return loose && typeof a !== "object" && typeof b !== "object" ? a == b : false;
    }
    if (!loose && Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
    if (tag(a) !== tag(b)) return false;
    if (a instanceof Date) return a.getTime() === b.getTime();
    if (a instanceof RegExp) return String(a) === String(b);
    if (a instanceof Error && (a.message !== b.message || a.name !== b.name)) return false;
    if (ArrayBuffer.isView(a)) {
      if (a.length !== b.length) return false;
      for (var i = 0; i < a.length; i++) if (!Object.is(a[i], b[i]) && !(loose && a[i] == b[i])) return false;
    } else if (a instanceof ArrayBuffer) {
      return deepEqual(new Uint8Array(a), new Uint8Array(b), loose);
    } else if (a instanceof Map) {
      if (a.size !== b.size) return false;
      var ok = true;
      a.forEach(function (v, k) { if (!b.has(k) || !deepEqual(v, b.get(k), loose)) ok = false; });
      if (!ok) return false;
    } else if (a instanceof Set) {
      if (a.size !== b.size) return false;
      var same = true;
      a.forEach(function (v) {
        if (b.has(v)) return;
        var found = false;
        b.forEach(function (w) { if (!found && typeof v === "object" && deepEqual(v, w, loose)) found = true; });
        if (!found) same = false;
      });
      if (!same) return false;
    }
    var ka = ownKeys(a);
    var kb = ownKeys(b);
    if (ka.length !== kb.length) return false;
    return ka.every(function (key) {
      return Object.prototype.hasOwnProperty.call(b, key) && deepEqual(a[key], b[key], loose);
    });
  }
  function checkError(error, expected, label) {
    if (expected === undefined) return;
    if (typeof expected === "function") {
      if (expected.prototype !== undefined && error instanceof expected) return;
      if (Error.isPrototypeOf(expected) || expected === Error) {
        fail(undefined, label + ": " + show(error) + " is not an instance of " + expected.name);
      }
      if (expected.call({}, error) !== true) fail(undefined, label + ": validation function returned false");
      return;
    }
    if (expected instanceof RegExp) {
      if (!expected.test(String(error))) fail(undefined, label + ": " + show(String(error)) + " does not match " + expected);
      return;
    }
    Reflect.ownKeys(expected).forEach(function (key) {
      var want = expected[key];
      var got = error == null ? undefined : error[key];
      if (want instanceof RegExp && typeof got === "string") {
        if (!want.test(got)) fail(undefined, label + ": " + String(key) + " " + show(got) + " does not match " + want);
      } else if (!deepEqual(got, want)) {
        fail(undefined, label + ": " + String(key) + " expected " + show(want) + ", got " + show(got));
      }
    });
  }
  var methods = {
    equal: function (a, b, m) { if (a != b && !(a !== a && b !== b)) fail(m, show(a) + " == " + show(b)); },
    notEqual: function (a, b, m) { if (a == b) fail(m, show(a) + " != " + show(b)); },
    strictEqual: function (a, b, m) { if (!Object.is(a, b)) fail(m, "expected " + show(b) + ", got " + show(a)); },
    notStrictEqual: function (a, b, m) { if (Object.is(a, b)) fail(m, "expected values to differ: " + show(a)); },
    deepEqual: function (a, b, m) { if (!deepEqual(a, b, true)) fail(m, "expected " + show(b) + ", got " + show(a)); },
    deepStrictEqual: function (a, b, m) { if (!deepEqual(a, b)) fail(m, "expected " + show(b) + ", got " + show(a)); },
    notDeepStrictEqual: function (a, b, m) { if (deepEqual(a, b)) fail(m, "expected values to differ: " + show(a)); },
    fail: function (m) { fail(m, "Failed"); },
    ifError: function (v) { if (v !== null && v !== undefined) throw v; },
    match: function (s, re, m) { if (!re.test(s)) fail(m, show(s) + " does not match " + re); },
    doesNotMatch: function (s, re, m) { if (re.test(s)) fail(m, show(s) + " matches " + re); },
    throws: function (fn, expected, m) {
      var threw = false;
      var error;
      try { fn(); } catch (e) { threw = true; error = e; }
      if (!threw) fail(typeof expected === "string" ? expected : m, "Missing expected exception");
      checkError(error, typeof expected === "string" ? undefined : expected, "throws");
    },
    doesNotThrow: function (fn) { fn(); },
    rejects: function (promiseOrFn, expected) {
      var p;
      try { p = typeof promiseOrFn === "function" ? promiseOrFn() : promiseOrFn; } catch (e) { return Promise.reject(e); }
      return Promise.resolve(p).then(
        function () { fail(undefined, "Missing expected rejection"); },
        function (error) { checkError(error, expected, "rejects"); },
      );
    },
    doesNotReject: function (promiseOrFn) {
      return Promise.resolve(typeof promiseOrFn === "function" ? promiseOrFn() : promiseOrFn).then(function () {});
    },
  };
  Object.keys(methods).forEach(function (key) { assert[key] = methods[key]; });
  assert.strict = assert;
  return assert;
})();
