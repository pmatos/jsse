// Runs a verbatim Node test file (test/parallel/test-event*.js, Node v26.8.2)
// unchanged on jsse --node and on Node: the runner prepends this file, wraps the
// test in `try { (function (require, __filename, __dirname) { ... })(...); }
// finally { __eventsFinish(); }` (a test may throw to an uncaughtException
// handler). It provides the pieces of `assert`, `test/common` and
// `node:test` the tests use. Process exit events are not relied on: expectations
// are verified after the test body and a quiet period.
var __eventsLoad = function (id) {
  return require(id);
};
var __eventsInspect = require("util").inspect;
if (typeof setImmediate === "undefined") {
  globalThis.setImmediate = function (fn) {
    return setTimeout.apply(null, [fn, 0].concat(Array.prototype.slice.call(arguments, 1)));
  };
}
if (typeof gc === "undefined" && typeof $262 === "object" && $262 !== null) {
  globalThis.gc = $262.gc;
}
var __eventsActivity = 0;
var __eventsPending = [];
var __eventsTests = [];

var __eventsAssert = (function () {
  function AssertionError(message) {
    var e = new Error(message);
    e.name = "AssertionError";
    e.code = "ERR_ASSERTION";
    return e;
  }
  function assert(value, message) {
    if (!value) throw AssertionError(message === undefined ? "assertion failed" : String(message));
  }
  assert.ok = assert;
  assert.AssertionError = AssertionError;
  function show(value) {
    try { return __eventsInspect(value); } catch (e) { return String(value); }
  }
  function fail(message, fallback) {
    if (message instanceof Error) throw message;
    throw AssertionError(message === undefined ? fallback : String(message));
  }
  function ownKeys(o) {
    return Reflect.ownKeys(o).filter(function (k) {
      return Object.prototype.propertyIsEnumerable.call(o, k);
    });
  }
  function deepEqual(a, b) {
    if (Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
    if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
    if (Object.prototype.toString.call(a) !== Object.prototype.toString.call(b)) return false;
    if (a instanceof Date) return a.getTime() === b.getTime();
    if (a instanceof RegExp) return String(a) === String(b);
    if (a instanceof Error && (a.message !== b.message || a.name !== b.name)) return false;
    if (a instanceof Map) {
      if (a.size !== b.size) return false;
      var ok = true;
      a.forEach(function (v, k) { if (!b.has(k) || !deepEqual(v, b.get(k))) ok = false; });
      return ok;
    }
    if (a instanceof Set) {
      if (a.size !== b.size) return false;
      var same = true;
      a.forEach(function (v) { if (!b.has(v)) same = false; });
      return same;
    }
    var ka = ownKeys(a);
    var kb = ownKeys(b);
    if (ka.length !== kb.length) return false;
    return ka.every(function (key) {
      return Object.prototype.hasOwnProperty.call(b, key) && deepEqual(a[key], b[key]);
    });
  }
  function matches(actual, expected) {
    if (expected instanceof RegExp) return expected.test(String(actual));
    return deepEqual(actual, expected);
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
    equal: function (a, b, m) { if (a != b) fail(m, show(a) + " == " + show(b)); },
    strictEqual: function (a, b, m) {
      if (!Object.is(a, b)) fail(m, "expected " + show(b) + ", got " + show(a));
    },
    notStrictEqual: function (a, b, m) {
      if (Object.is(a, b)) fail(m, "expected values to differ: " + show(a));
    },
    deepStrictEqual: function (a, b, m) {
      if (!deepEqual(a, b)) fail(m, "expected " + show(b) + ", got " + show(a));
    },
    notDeepStrictEqual: function (a, b, m) {
      if (deepEqual(a, b)) fail(m, "expected values to differ: " + show(a));
    },
    fail: function (m) { fail(m, "Failed"); },
    ifError: function (v) { if (v !== null && v !== undefined) throw v; },
    match: function (s, re, m) { if (!re.test(s)) fail(m, show(s) + " does not match " + re); },
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

function __eventsMustCall(fn, exact, atLeast) {
  if (typeof fn === "number") { exact = fn; fn = undefined; }
  if (exact === undefined) exact = 1;
  var expectation = { name: fn && fn.name || "<anonymous>", exact: exact, atLeast: atLeast, actual: 0, stack: new Error().stack };
  __eventsPending.push(expectation);
  return function () {
    expectation.actual++;
    __eventsActivity++;
    return fn ? fn.apply(this, arguments) : undefined;
  };
}

var __eventsCommon = {
  isWindows: false,
  hasCrypto: false,
  skip: function (reason) {
    console.log("1..0 # Skipped: " + reason);
    process.exit(0);
  },
  platformTimeout: function (ms) { return ms; },
  mustCall: function (fn, exact) { return __eventsMustCall(fn, exact, false); },
  mustCallAtLeast: function (fn, minimum) { return __eventsMustCall(fn, minimum, true); },
  mustNotCall: function (msg) {
    return function mustNotCall() {
      __eventsActivity++;
      var e = new Error(msg || "function should not have been called");
      __eventsFailures.push(e);
      throw e;
    };
  },
  mustSucceed: function (fn, exact) {
    return __eventsMustCall(function (err) {
      __eventsAssert.ifError(err);
      if (typeof fn === "function") return fn.apply(this, Array.prototype.slice.call(arguments, 1));
    }, exact, false);
  },
  expectsError: function (validator, exact) {
    return __eventsMustCall(function (error) {
      __eventsAssert.throws(function () { throw error; }, validator);
      return true;
    }, exact, false);
  },
  expectWarning: function (nameOrMap, expected, code) {
    var map = nameOrMap;
    if (typeof nameOrMap === "string") {
      map = {};
      map[nameOrMap] = code === undefined ? [expected] : [[expected, code]];
    }
    var table = {};
    Object.keys(map).forEach(function (name) {
      var list = Array.isArray(map[name]) ? map[name] : [map[name]];
      table[name] = list.map(function (entry) { return Array.isArray(entry) ? entry : [entry]; });
    });
    var count = 0;
    Object.keys(table).forEach(function (name) { count += table[name].length; });
    process.on("warning", __eventsMustCall(function (warning) {
      var entries = table[warning.name];
      __eventsAssert.ok(entries && entries.length, "unexpected warning " + warning.name);
      var entry = entries.shift();
      __eventsAssert.strictEqual(warning.message, entry[0]);
      if (entry[1] !== undefined) __eventsAssert.strictEqual(warning.code, entry[1]);
    }, count, false));
  },
  invalidArgTypeHelper: function (input) {
    if (input == null) return " Received " + input;
    if (typeof input === "function") return " Received function " + input.name;
    if (typeof input === "object") {
      if (input.constructor && input.constructor.name) {
        return " Received an instance of " + input.constructor.name;
      }
      return " Received " + __eventsInspect(input, { depth: -1 });
    }
    var inspected = __eventsInspect(input, { colors: false });
    if (inspected.length > 28) inspected = inspected.slice(0, 25) + "...";
    return " Received type " + typeof input + " (" + inspected + ")";
  },
};
var __eventsFailures = [];

// Subtests jsse cannot run yet, skipped on jsse only (they pass on Node).
var __eventsJsseSkips = {
  "Does not hang forever": "jsse has no --input-type flag, so the spawned child cannot read an ES module from stdin",
  "Aborted with gc cleanup": "$262.gc() does not collect WeakRef targets (#1317)",
  "AbortSignal.timeout() does not prevent the signal from being collected": "$262.gc() does not collect WeakRef targets (#1317)",
  "AbortSignal with a timeout is not collected while there is an active listener": "$262.gc() does not collect WeakRef targets (#1317)",
};
var __eventsNodeTest = (function () {
  function register(name, options, fn) {
    if (typeof options === "function") { fn = options; options = {}; }
    var skip = options && options.skip ||
      (typeof __host_write === "function" && Object.prototype.hasOwnProperty.call(__eventsJsseSkips, name));
    __eventsTests.push({ name: name, fn: fn, skip: skip });
  }
  function describe(name, fn) { fn(); }
  register.mock = {
    fn: function (impl) {
      var f = function () {
        f.mock.calls.push({ arguments: Array.prototype.slice.call(arguments) });
        return impl ? impl.apply(this, arguments) : undefined;
      };
      f.mock = { calls: [] };
      return f;
    },
  };
  register.test = register;
  register.it = register;
  register.describe = describe;
  return register;
})();

var __eventsRequire = function (id) {
  switch (id.replace(/^node:/, "")) {
    case "../common": return __eventsCommon;
    case "assert": return __eventsAssert;
    case "events": return __eventsLoad("events");
    case "test": return __eventsNodeTest;
  }
  return require(id);
};

function __eventsFail(message) {
  console.error(message);
  process.exit(1);
}

function __eventsRunTests() {
  var chain = Promise.resolve();
  __eventsTests.forEach(function (t) {
    chain = chain.then(function () {
      if (t.skip) return;
      __eventsActivity++;
      return Promise.resolve().then(function () { return t.fn({ diagnostic: function () {}, after: function () {} }); })
        .catch(function (e) { __eventsFailures.push(e); });
    });
  });
  return chain;
}

function __eventsVerify() {
  var problems = __eventsFailures.map(function (e) { return String(e && e.stack || e); });
  __eventsPending.forEach(function (e) {
    var good = e.atLeast ? e.actual >= e.exact : e.actual === e.exact;
    if (!good) problems.push("mustCall " + e.name + ": expected " + (e.atLeast ? "at least " : "") + e.exact + " calls, got " + e.actual + "\n" + e.stack);
  });
  if (problems.length) return __eventsFail(problems.join("\n"));
  console.log("events-fixture: ok");
  process.exit(0);
}

function __eventsFinish() {
  __eventsRunTests().then(function () {
    var quiet = 0;
    var last = -1;
    (function poll() {
      if (last === __eventsActivity) {
        if (++quiet >= 3) return __eventsVerify();
      } else {
        quiet = 0;
        last = __eventsActivity;
      }
      setTimeout(poll, 20);
    })();
  }, __eventsFail);
}
