// CommonJS selector used by tape-based library bundles.
//
// scripts/node-test-harness.js installs __tape only under JSSE's --node host
// mode. Real Node therefore loads upstream tape from the cloned library's
// node_modules, retaining an independent framework oracle for the same bundle.

if (typeof globalThis.__tape === "function") {
  module.exports = globalThis.__tape;
} else {
  module.exports = require("../node_modules/tape");
}
