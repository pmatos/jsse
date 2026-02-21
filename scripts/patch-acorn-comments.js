// Patch acorn's TestComments test for esbuild compatibility.
//
// esbuild strips comments from function bodies, but TestComments relies on
// Function.prototype.toString() preserving them. This script replaces the
// `.toString().replace(...)` pattern with a string literal containing the
// original source, so comments survive bundling.

const fs = require("fs");
const file = process.argv[2];
if (!file) {
  console.error("Usage: node patch-acorn-comments.js <tests.js>");
  process.exit(1);
}

let src = fs.readFileSync(file, "utf8");

const startMarker = "test(function TestComments()";
const startIdx = src.indexOf(startMarker);
if (startIdx === -1) {
  console.log("TestComments not found, skipping patch");
  process.exit(0);
}

// Find the closing: `}.toString().replace(/\r\n/g, '\n'),`
const endMarker = "}.toString().replace(/\\r\\n/g, '\\n'),";
const endIdx = src.indexOf(endMarker, startIdx);
if (endIdx === -1) {
  console.log("toString pattern not found, skipping patch");
  process.exit(0);
}

// Extract the function source (from "function TestComments()..." to "}")
const fnStart = startIdx + "test(".length;
const fnEnd = endIdx + 1; // include the closing }
const fnSrc = src.substring(fnStart, fnEnd);

// Build the replacement: test(<string literal>,
const replacement = "test(" + JSON.stringify(fnSrc) + ",";

// Replace the original range: from "test(function TestComments..." to the comma after .replace(...)
const originalEnd = endIdx + endMarker.length;
src = src.substring(0, startIdx) + replacement + src.substring(originalEnd);

fs.writeFileSync(file, src);
console.log("TestComments patched successfully");
