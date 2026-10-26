// Second half of the test prelude (after harness-assert.js): the pieces of
// `test/common` the stream tests use, `mustCall` bookkeeping verified when the
// process exits, and the `require` the test body is given. A test passes when
// the process exits 0 and prints "stream-fixture: ok".
var __sPending = [];

function __sMustCall(fn, exact, atLeast) {
  if (typeof fn === "number") { exact = fn; fn = undefined; }
  if (exact === undefined) exact = 1;
  var e = { name: (fn && fn.name) || "<anonymous>", exact: exact, atLeast: atLeast, actual: 0, stack: new Error().stack };
  __sPending.push(e);
  return function () {
    e.actual++;
    return fn ? fn.apply(this, arguments) : undefined;
  };
}

var __sCommon = {
  isWindows: false,
  isMainThread: true,
  hasCrypto: false,
  hasIntl: false,
  skip: function (reason) {
    console.log("1..0 # Skipped: " + reason);
    process.exit(0);
  },
  platformTimeout: function (ms) { return ms; },
  mustCall: function (fn, exact) { return __sMustCall(fn, exact, false); },
  mustCallAtLeast: function (fn, minimum) { return __sMustCall(fn, minimum, true); },
  mustNotCall: function (msg) {
    return function mustNotCall() {
      throw new Error(msg || "function should not have been called");
    };
  },
  mustSucceed: function (fn, exact) {
    return __sMustCall(function (err) {
      __sAssert.ifError(err);
      if (typeof fn === "function") return fn.apply(this, Array.prototype.slice.call(arguments, 1));
    }, exact, false);
  },
  expectsError: function (validator, exact) {
    return __sMustCall(function (error) {
      __sAssert.throws(function () { throw error; }, validator);
      return true;
    }, exact, false);
  },
  getArrayBufferViews: function (buf) {
    var buffer = buf.buffer, byteOffset = buf.byteOffset, byteLength = buf.byteLength;
    var out = [];
    [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array,
      Float32Array, Float64Array, DataView].forEach(function (Type) {
      var size = Type.BYTES_PER_ELEMENT || 1;
      if (byteLength % size === 0) out.push(new Type(buffer, byteOffset, byteLength / size));
    });
    return out;
  },
  getBufferSources: function (buf) {
    return [].concat(__sCommon.getArrayBufferViews(buf), [new Uint8Array(buf).buffer]);
  },
  invalidArgTypeHelper: function (input) {
    if (input == null) return " Received " + input;
    if (typeof input === "function") return " Received function " + input.name;
    if (typeof input === "object") {
      if (input.constructor && input.constructor.name) return " Received an instance of " + input.constructor.name;
      return " Received " + __sInspect(input, { depth: -1 });
    }
    var inspected = __sInspect(input, { colors: false });
    if (inspected.length > 28) inspected = inspected.slice(0, 25) + "...";
    return " Received type " + typeof input + " (" + inspected + ")";
  },
};

var __sRequire = function (id) {
  switch (id.replace(/^node:/, "")) {
    case "../common": return __sCommon;
    case "assert":
    case "assert/strict": return __sAssert;
    case "stream":
    case "stream/promises":
    case "events":
    case "util":
    case "timers/promises": return require(id);
  }
  throw new Error("test requires unsupported module " + id);
};

process.on("exit", function (code) {
  if (code !== 0) return;
  var problems = [];
  __sPending.forEach(function (e) {
    var good = e.atLeast ? e.actual >= e.exact : e.actual === e.exact;
    if (!good) {
      problems.push("mustCall " + e.name + ": expected " + (e.atLeast ? "at least " : "") + e.exact + " calls, got " + e.actual + "\n" + e.stack);
    }
  });
  if (problems.length) {
    console.error(problems.join("\n"));
    process.exitCode = 1;
    return;
  }
  console.log("stream-fixture: ok");
});
