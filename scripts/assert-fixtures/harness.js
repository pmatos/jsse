// Prepended to a verbatim Node test (test/parallel/test-assert*.js, v26.8.2) by
// scripts/run-node-assert-oracle.sh. The same text runs on jsse --node and on
// Node, with the real `assert` module under test on both. It supplies the
// pieces of `test/common`, `test/common/fixtures` and `node:test` the assert
// tests use. A test passes when the process exits 0 and prints
// "assert-fixture: ok". ASSERT_FIXTURE_SKIP lists test names (one per line)
// that are reported as skipped (the runner sets it for jsse only).
var __aUtil = require("util");
var __aSkip = (process.env.ASSERT_FIXTURE_SKIP || "").split("\n").filter(Boolean);
var __aPending = [];

function __aMustCall(fn, exact, atLeast) {
  if (typeof fn === "number") { exact = fn; fn = undefined; }
  if (exact === undefined) exact = 1;
  var e = { name: (fn && fn.name) || "<anonymous>", exact: exact, atLeast: atLeast, actual: 0, stack: new Error().stack };
  __aPending.push(e);
  return function () {
    e.actual++;
    return fn ? fn.apply(this, arguments) : undefined;
  };
}

var __aCommon = {
  isWindows: false,
  isMainThread: true,
  hasCrypto: false,
  hasIntl: false,
  hasLocalStorage: false,
  mustCall: function (fn, exact) { return __aMustCall(fn, exact, false); },
  mustCallAtLeast: function (fn, minimum) { return __aMustCall(fn, minimum, true); },
  mustNotCall: function (msg) {
    return function mustNotCall() {
      throw new Error(msg || "function should not have been called");
    };
  },
  expectsError: function (validator, exact) {
    return __aMustCall(function (error) {
      require("assert").throws(function () { throw error; }, validator);
      return true;
    }, exact, false);
  },
  invalidArgTypeHelper: function (input) {
    if (input == null) return " Received " + input;
    if (typeof input === "function") return " Received function " + input.name;
    if (typeof input === "object") {
      if (input.constructor && input.constructor.name) return " Received an instance of " + input.constructor.name;
      return " Received " + __aUtil.inspect(input, { depth: -1 });
    }
    var inspected = __aUtil.inspect(input, { colors: false });
    if (inspected.length > 28) inspected = inspected.slice(0, 25) + "...";
    return " Received type " + typeof input + " (" + inspected + ")";
  },
};

var __aFixtures = {
  path: function (name) { return process.env.ASSERT_FIXTURE_DIR + "/fixtures/" + name; },
};

var __aQueue = [];
var __aStack = [];

function __aRegister(args, kind) {
  var name = "<anonymous>";
  var opts = {};
  var fn;
  for (var i = 0; i < args.length; i++) {
    if (typeof args[i] === "string") name = args[i];
    else if (typeof args[i] === "function") fn = args[i];
    else if (args[i] && typeof args[i] === "object") opts = args[i];
  }
  var node = { name: __aStack.concat(name).join(" > "), opts: opts, fn: fn, kind: kind, children: [] };
  var parent = __aStack.length ? __aCurrent : null;
  (parent ? parent.children : __aQueue).push(node);
  if (kind === "suite" && fn && !opts.skip) {
    var saved = __aCurrent;
    __aStack.push(name);
    __aCurrent = node;
    try { fn(); } finally { __aStack.pop(); __aCurrent = saved; }
  }
}
var __aCurrent = null;

var __aTestModule = {
  test: function () { __aRegister(arguments, "test"); },
  describe: function () { __aRegister(arguments, "suite"); },
  suite: function () { __aRegister(arguments, "suite"); },
  it: function () { __aRegister(arguments, "test"); },
};

var __aCounts = { pass: 0, fail: 0, skip: 0 };

async function __aRun(nodes) {
  for (var i = 0; i < nodes.length; i++) {
    var n = nodes[i];
    var skipped = n.opts.skip || __aSkip.some(function (s) { return n.name.indexOf(s) !== -1; });
    if (skipped) { __aCounts.skip++; console.log("skip " + n.name); continue; }
    if (n.kind === "suite") { await __aRun(n.children); continue; }
    try {
      await n.fn({ test: function () {} });
      __aCounts.pass++;
    } catch (e) {
      __aCounts.fail++;
      console.log("FAIL " + n.name + "\n" + (e && e.stack || e));
    }
  }
}

var __aVm;
try {
  __aVm = require("vm");
} catch (e) {
  __aVm = {
    runInNewContext: function (code) { return (0, eval)(code); },
  };
  ["runInContext", "createContext", "Script"].forEach(function (k) {
    __aVm[k] = function () { throw new Error("vm." + k + " is not available"); };
  });
}

// Stand-ins for the web globals jsse does not have yet; the tests that name them
// only compare the classes' prototype chains.
if (typeof Blob === "undefined") globalThis.Blob = class Blob {};
if (typeof File === "undefined") globalThis.File = class File extends Blob {};

var __aRequire = function (id) {
  switch (id.replace(/^node:/, "")) {
    case "../common": return __aCommon;
    case "../common/fixtures": return __aFixtures;
    case "test": return __aTestModule;
    case "vm": return __aVm;
  }
  return require(id);
};

process.on("exit", function (code) {
  if (code !== 0) return;
  var problems = [];
  __aPending.forEach(function (e) {
    var good = e.atLeast ? e.actual >= e.exact : e.actual === e.exact;
    if (!good) {
      problems.push("mustCall " + e.name + ": expected " + (e.atLeast ? "at least " : "") + e.exact + " calls, got " + e.actual + "\n" + e.stack);
    }
  });
  if (__aCounts.fail > 0) problems.push(__aCounts.fail + " test(s) failed");
  if (problems.length) {
    console.error(problems.join("\n"));
    process.exitCode = 1;
    return;
  }
  console.log("assert-fixture: ok " + __aCounts.pass + " passed, " + __aCounts.skip + " skipped");
});

function __aStart() {
  setImmediate(function () {
    __aRun(__aQueue).catch(function (e) { console.error(e); process.exitCode = 1; });
  });
}
