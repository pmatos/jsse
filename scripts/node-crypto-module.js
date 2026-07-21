// CommonJS `crypto` selector for bundled library dependencies.
//
// Under esbuild's platform=node bundling, an unaliased `require("crypto")`
// compiles to a lazy external-require helper that throws "Dynamic require of
// crypto is not supported" once evaluated in a non-Node host — the module
// resolves but the *call* still fails at runtime. Aliasing the specifier keeps
// dependents (e.g. TweetNaCl's own PRNG auto-init) on a real, statically
// bundled module instead. Only randomBytes is needed; the shared
// __host_random_bytes syscall floor (issue #229) backs it on JSSE. Node keeps
// using its native module.

if (typeof __host_write !== "undefined") {
  module.exports = {
    randomBytes: function randomBytes(size) {
      return globalThis.Buffer.from(__host_random_bytes(size));
    },
  };
} else {
  module.exports = require("node:crypto");
}
