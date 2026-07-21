// Vendors the real "assert" npm package (browserify's pure-JS port of Node's
// assert module, pinned in lib_prepare) so jsse's "node:assert/strict"
// resolution (see uuid-jsse-require-shim.js) has real assertion semantics
// instead of a hand-rolled reimplementation. Node itself never reads this —
// it resolves "node:assert/strict" to its own native module.
//
// The relative specifier below (as opposed to the bare name "assert") is
// deliberate: esbuild's --platform=node build automatically treats bare
// Node-builtin-shaped specifiers (including plain "assert") as external and
// leaves them unresolved, which is exactly what we want for the *test files'*
// "node:assert/strict" imports but not for this vendor copy. A concrete
// relative path sidesteps that heuristic and gets fully bundled, the same
// trick node-tape-module.js uses for tape.
// Copied into dist/ by lib_prepare, one level below the repo root where
// node_modules lives.
const assertPkg = require("../node_modules/assert");
globalThis.__jsseAssertStrict = assertPkg.strict;
