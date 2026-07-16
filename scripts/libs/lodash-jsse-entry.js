// jsse bundle entry for lodash's QUnit test suite (copied into the cloned repo
// as test/jsse-entry.js by lodash.sh's lib_prepare).
//
// lodash/test/test.js loads the module-under-test and its `ui` metadata via
// *dynamic* require() calls — `require(filePath)` and `'default' in
// require(filePath)` — which esbuild cannot bundle (it emits a __require shim
// that throws under jsse, where there is no global `require`). We pre-populate
// the two globals test.js probes so those dynamic-require branches
// short-circuit:
//
//   * `root._`  — the lodash under test (a *static* require, so esbuild bundles
//                 it), which satisfies `root._ || (root._ = require(filePath))`.
//   * `root.ui` — the build metadata object, which satisfies
//                 `root.ui || (root.ui = { … 'default' in require(filePath) … })`.
//
// The stable lodash is loaded by test.js as
// `interopRequire('../node_modules/lodash/lodash.js')`, which is *also* dynamic
// (interopRequire does `require(id)` on its argument), so we pre-set
// `root.lodashStable` too — from a *static* `require("lodash")` that esbuild
// bundles (the pinned 4.17.20 stable installed by lodash.sh's lib_prepare).
//
// `qunit-extras` is loaded by test.js via a static string require, so esbuild
// bundles it without help. On jsse the node-test-harness.js prelude installs a
// global `QUnit`, so `root.QUnit || require('qunit-extras')` uses our adapter
// and the bundled qunit-extras stays dormant; on Node the prelude is inert, so
// real qunit-extras runs as the reference oracle.
//
// `global` (Node's alias for the global object) is provided by the prelude, so
// test.js's `root = (typeof global == 'object' && global) || this` resolves to
// the same object we set properties on here.
var _ = require("../lodash.js");
if (_ && _.default) _ = _.default;

var stable = require("lodash");
if (stable && stable.default) stable = stable.default;

var root =
  (typeof globalThis === "object" && globalThis) ||
  (typeof global === "object" && global) ||
  this;

root._ = _;
root.lodashStable = stable;
root.ui = {
  buildPath: "../lodash.js",
  loaderPath: "",
  isModularize: false,
  isStrict: false,
  urlParams: {},
};

require("./test.js");
