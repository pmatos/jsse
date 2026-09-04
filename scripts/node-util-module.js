// CommonJS `util` selector for bundled library dependencies.
//
// node-shim.js exposes its JS-only format/inspect implementation as
// globalThis.util on JSSE. Node keeps using the complete native module.

if (typeof __host_write !== "undefined") {
  module.exports = globalThis.util;
} else {
  module.exports = require("node:util");
}
