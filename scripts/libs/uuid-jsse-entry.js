// Bundle entry for uuid's upstream node:test suite. Copied into the built
// browser distribution (dist/) by lib_prepare, alongside
// uuid-assert-connector.js, so the relative imports below resolve against the
// pure-JS md5/sha1 + crypto.getRandomValues browser build rather than the
// node:crypto-backed Node build. Every *.test.js here is upstream, unmodified.
import "./uuid-assert-connector.js";
import "./test/parse.test.js";
import "./test/rng.test.js";
import "./test/stringify.test.js";
import "./test/v1.test.js";
import "./test/v35.test.js";
import "./test/v4.test.js";
import "./test/v6.test.js";
import "./test/v7.test.js";
import "./test/validate.test.js";
import "./test/version.test.js";
