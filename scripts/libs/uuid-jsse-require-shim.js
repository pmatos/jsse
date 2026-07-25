// Resolves the two Node builtins uuid's upstream test files import directly —
// "node:test" and "node:assert/strict" — for jsse only, so the identical
// bundle keeps running its *unmodified* upstream tests against real Node
// natively (issue #302's design bar: "Node uses its native modules").
//
// esbuild's --platform=node build automatically treats Node-builtin-shaped
// specifiers (including the "node:" prefix form) as external, compiling each
// import down to a literal `require(specifier)` call via esbuild's internal
// __require helper — which falls back to whatever `require` identifier is in
// scope when the bundle IIFE executes. On real Node that identifier is the
// module's own CJS-wrapper `require` (visible from a nested IIFE via
// closure), which resolves both specifiers natively — this shim must stay out
// of its way there. On jsse there is no such ambient `require`, so this shim
// installs one as a global, keyed to exactly these two specifiers.
//
// Must load after node-test-harness.js (this file reads its describe/test/
// before/after/beforeEach/afterEach globals) and requires
// uuid-assert-connector.js to have run (read lazily at call time, so load
// order relative to that one doesn't matter — it's bundled into the main
// entry, not a prelude shim).
(function () {
  "use strict";

  // Detecting real Node via process.versions.node would be wrong here:
  // node-shim.js (loaded earlier) installs a *fake* process with
  // versions.node set, to pass UMD-style library checks off as real Node.
  // __host_write is the #229 syscall floor, present only under jsse's
  // --node host mode and never on real Node — the same signal
  // node-test-harness.js keys off for the identical reason.
  if (typeof __host_write === "undefined") return;

  var harnessDescribe = globalThis.describe;
  var harnessTest = globalThis.test;
  var harnessBefore = globalThis.before;
  var harnessAfter = globalThis.after;
  var harnessBeforeEach = globalThis.beforeEach;
  var harnessAfterEach = globalThis.afterEach;

  // node:test invokes a test/it callback with a TestContext `t` as its first
  // argument whenever the callback declares at least one parameter — a
  // different convention from the shared TAP harness's mocha/tape-style
  // single-arg "done callback" tests (see node-test-harness.js's
  // invokeRunnable). Registering an arity-0 wrapper with the harness
  // sidesteps that ambiguity entirely (invokeRunnable always takes the
  // promise-based branch, awaiting whatever the real callback returns) while
  // still handing the real callback a TestContext-shaped `t`.
  //
  // uuid's suite only ever touches `t.mock.method`/`t.mock.reset` (see
  // v4.test.ts's native-crypto.randomUUID() tests), so that's the only
  // TestContext surface implemented here.
  function makeTestContext() {
    var patches = [];
    return {
      mock: {
        method: function (obj, methodName, impl) {
          var original = obj[methodName];
          var callCount = 0;
          function mockFn() {
            callCount++;
            return impl.apply(this, arguments);
          }
          mockFn.mock = {
            callCount: function () {
              return callCount;
            },
          };
          patches.push({
            obj: obj,
            methodName: methodName,
            original: original,
          });
          obj[methodName] = mockFn;
          return mockFn;
        },
        reset: function () {
          for (var i = 0; i < patches.length; i++) {
            patches[i].obj[patches[i].methodName] = patches[i].original;
          }
          patches.length = 0;
        },
      },
    };
  }

  function wrapRunnable(fn) {
    if (typeof fn !== "function" || fn.length === 0) return fn;
    return function () {
      return fn(makeTestContext());
    };
  }

  function test(name, fn) {
    return harnessTest(name, wrapRunnable(fn));
  }

  var nodeTestModule = {
    describe: harnessDescribe,
    test: test,
    it: test,
    before: harnessBefore,
    after: harnessAfter,
    beforeEach: harnessBeforeEach,
    afterEach: harnessAfterEach,
  };

  globalThis.require = function (specifier) {
    if (specifier === "node:test") return nodeTestModule;
    if (specifier === "node:assert/strict") {
      return globalThis.__jsseAssertStrict;
    }
    throw new Error("jsse require shim: unsupported specifier " + specifier);
  };
})();
