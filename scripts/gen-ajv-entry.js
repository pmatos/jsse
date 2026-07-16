// Generate AJV's single-file validation-corpus entry.
//
// On jsse, node-test-harness.js has already installed Mocha-shaped globals.
// On Node, this entry starts real Mocha programmatically before loading the
// exact same statically bundled spec. The summary shape is shared so the
// generalized runner can cross-check the registered test count.

"use strict";

var fs = require("fs");

var output = process.argv[2];
if (!output) {
  console.error("usage: node gen-ajv-entry.js <output>");
  process.exit(2);
}

var source = String.raw`var runningOnJsse = typeof __host_write !== "undefined";
var mocha;

if (!runningOnJsse) {
  var Mocha = require("mocha");
  mocha = new Mocha({reporter: "dot"});
  mocha.suite.emit("pre-require", globalThis, "jsse-ajv-entry", mocha);
} else if (typeof __dirname === "undefined") {
  // AJV passes __dirname to json-schema-test, but the generated fixture lists
  // are already arrays of inlined JSON objects and never use it for discovery.
  globalThis.__dirname = "";
}

require("./json-schema.spec.ts");

if (!runningOnJsse) {
  var total = mocha.suite.total();
  mocha.run(function (failures) {
    console.log(
      "    PASS: " + (total - failures) +
      "  FAIL: " + failures +
      "  TOTAL: " + total
    );
    process.exitCode = failures ? 1 : 0;
  });
}
`;

fs.writeFileSync(output, source);
console.log("gen-ajv-entry: wrote " + output);
