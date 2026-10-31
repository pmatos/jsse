// Third part of the web-streams test prelude, after the stream fixtures'
// harness-assert.js and harness.js (scripts/stream-fixtures): `node:test`
// (sequential, enough for the vendored tests), the extra pieces of
// `test/common` they use, and a `require` that also reaches the modules the
// web-stream tests import. A test passes when the process exits 0 and prints
// "stream-fixture: ok"; every `test()` counts as a mustCall, so one whose
// promise never settles fails at exit instead of passing.
var __wsQueue = Promise.resolve();

function __wsFail(name, error) {
  console.error("not ok " + name + "\n" + (error && error.stack || error));
  process.exitCode = 1;
}

function __wsTest(name, options, fn) {
  if (typeof name === "function") { fn = name; name = fn.name; options = undefined; }
  if (typeof options === "function") { fn = options; options = undefined; }
  var finished = __sMustCall(function () {}, 1);
  var context = { name: name, test: __wsSubtest, diagnostic: function () {} };
  function run() {
    if (options && options.skip) { finished(); return undefined; }
    return Promise.resolve().then(function () {
      return fn(context);
    }).then(finished, function (error) { __wsFail(name, error); });
  }
  __wsQueue = __wsQueue.then(run);
  return __wsQueue;
}

function __wsSubtest(name, options, fn) {
  if (typeof options === "function") { fn = options; options = undefined; }
  return Promise.resolve().then(function () { return fn({ name: name, test: __wsSubtest }); });
}

function __wsSuite(name, fn) {
  if (typeof name === "function") fn = name;
  fn();
}

__sCommon.hasIntl = true;
__sCommon.expectWarning = function () {};

__sRequire = function (id) {
  var bare = id.replace(/^node:/, "");
  if (bare === "test") {
    __wsTest.test = __wsTest;
    __wsTest.it = __wsTest;
    __wsTest.suite = __wsSuite;
    __wsTest.describe = __wsSuite;
    return __wsTest;
  }
  if (bare === "../common") return __sCommon;
  if (bare === "util/types") return require("util").types;
  return require(id);
};
