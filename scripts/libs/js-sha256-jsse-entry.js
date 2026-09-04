// jsse bundle entry for js-sha256's deterministic SHA-224/SHA-256 and HMAC
// vectors. The upstream Node entry repeats these files under several module
// loader configurations by evicting require.cache, which is not meaningful
// after esbuild has bundled every module once. Load the vector files once here
// while preserving all of their string, Array, Buffer, TypedArray, and
// ArrayBuffer cases.

var runningOnJsse = typeof __host_write !== "undefined";
var mocha;

if (runningOnJsse) {
  // node-test-harness.js provides describe/it but js-sha256 also uses Mocha's
  // context alias.
  globalThis.context = globalThis.describe;
} else {
  // The shared harness is deliberately inert on Node. Install real Mocha's
  // globals so Node remains an independent framework oracle.
  var Mocha = require("mocha");
  mocha = new Mocha();
  mocha.suite.emit("pre-require", globalThis, "tests/jsse-entry.js", mocha);
}

// The vector files use exactly two expect.js operations. Upstream's expect.js
// 0.3.1 mutates Function#length while installing its fluent chain; that was a
// silent no-op in its original sloppy CommonJS wrapper but throws after esbuild
// places it in strict code. Keep the upstream cases unchanged and provide their
// small assertion seam directly.
globalThis.expect = function (actual) {
  var chain = {
    be: function (expected) {
      if (actual !== expected) {
        throw new Error("expected " + actual + " to be " + expected);
      }
    },
    throwError: function (pattern) {
      if (typeof actual !== "function") {
        throw new Error("expected a function");
      }
      try {
        actual();
      } catch (error) {
        var message =
          error && typeof error.message !== "undefined"
            ? String(error.message)
            : String(error);
        if (!pattern || pattern.test(message)) return;
        throw new Error("expected error " + message + " to match " + pattern);
      }
      throw new Error("expected function to throw");
    },
  };
  return { to: chain };
};

// node-shim.js advertises process.versions.node, but this target is intended to
// exercise js-sha256's own implementation rather than its native crypto fast
// path. This is the same browser/webpack mode used by upstream's Node entry.
globalThis.window = globalThis;
globalThis.JS_SHA256_NO_NODE_JS = true;

var hashes = require("../src/sha256.js");
globalThis.sha256 = hashes.sha256;
globalThis.sha224 = hashes.sha224;
globalThis.BUFFER = true;

require("./test.js");
require("./hmac-test.js");

if (!runningOnJsse) {
  var runner = mocha.run(function (failures) {
    console.log(
      "    PASS: " +
        runner.stats.passes +
        "  FAIL: " +
        runner.stats.failures +
        "  TOTAL: " +
        runner.stats.tests
    );
    process.exitCode = failures ? 1 : 0;
  });
}
