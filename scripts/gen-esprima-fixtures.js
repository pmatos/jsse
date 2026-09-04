// Run once from esprima.sh's lib_prepare, cwd'd into the cloned esprima repo.
//
// Emits a bundlable data module (plain `module.exports = {...}` object
// literal, no fs at runtime) so esprima-jsse-entry.js's grammar-smoke describe
// block doesn't need `require('fs')`, which esbuild would otherwise leave as
// an unresolved external require at bundle runtime.

var fs = require("fs");
var path = require("path");

// This script runs cwd'd into the cloned esprima repo (see esprima.sh's
// lib_prepare); resolve output paths and everything.js against that cwd, not
// against this script's own location under jsse's scripts/.
var repoRoot = process.cwd();

function writeModule(relPath, data) {
  fs.writeFileSync(
    path.join(repoRoot, relPath),
    "module.exports = " + JSON.stringify(data) + ";\n"
  );
  console.log("wrote", relPath);
}

// test/grammar-tests.js: everything.js's es2015-script/module fixtures.
writeModule("test/jsse-everything-fixtures.js", {
  script: fs.readFileSync(
    require.resolve("everything.js/es2015-script", { paths: [repoRoot] }),
    "utf-8"
  ),
  module: fs.readFileSync(
    require.resolve("everything.js/es2015-module", { paths: [repoRoot] }),
    "utf-8"
  ),
});
