// Preserve Moment's deprecation test semantics after bundling.
//
// Upstream transpiles each test independently, so the deprecate helper imported
// by this test has a different hooks module from the Moment instance configured
// by the test lifecycle. esbuild correctly deduplicates that module in the
// single-file harness bundle. Explicitly expecting the deprecation keeps the
// original behavior meaningful without suppressing it or changing test counts.

"use strict";

const fs = require("fs");

const file = "src/test/moment/deprecate.js";
const source = fs.readFileSync(file, "utf8");
const before = `    // NOTE: hooks inside deprecate.js and moment are different, so this is can
    // not be test.expectedDeprecations(...)
    var fn = function () {},`;
const after = `    // The single-file harness bundle shares the hooks module with Moment.
    test.expectedDeprecations('testing deprecation');
    var fn = function () {},`;
const first = source.indexOf(before);

if (first === -1) {
  throw new Error(`patch-moment-bundle: no match in ${file}`);
}
if (source.indexOf(before, first + before.length) !== -1) {
  throw new Error(`patch-moment-bundle: ambiguous match in ${file}`);
}

fs.writeFileSync(
  file,
  source.slice(0, first) + after + source.slice(first + before.length)
);
console.log("patch-moment-bundle: made the deprecation expectation explicit");
