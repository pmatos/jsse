// Runs a file in node as a classic script (not a CommonJS module), so
// top-level `this`, `var` bindings and function declarations land on the
// global object the way they do in jsse. Used by scripts/run-differential.py.
"use strict";
const fs = require("fs");
const vm = require("vm");
require("./node-test262-prelude.js");
const file = process.argv[2];
vm.runInThisContext(fs.readFileSync(file, "utf8"), { filename: file });
