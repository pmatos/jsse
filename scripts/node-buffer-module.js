// CommonJS `buffer` selector for bundled library dependencies.
//
// The shared Buffer shim installs globalThis.Buffer only on JSSE. Node keeps
// using its native module, so dependencies such as safer-buffer see their usual
// API on the reference path.

if (typeof __host_write !== "undefined") {
  module.exports = {
    Buffer: globalThis.Buffer,
    SlowBuffer: function SlowBuffer(size) {
      return globalThis.Buffer.alloc(size);
    },
  };
} else {
  module.exports = require("node:buffer");
}
