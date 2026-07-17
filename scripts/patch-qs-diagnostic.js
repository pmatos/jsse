// Keep qs's strict-assignment assertion while accepting JSSE's shorter
// TypeError diagnostic. ECMAScript specifies the throw, not its message; the
// Node path continues to use qs's unchanged Node-specific regular expression.

const fs = require("fs");

const file = process.argv[2];
if (!file) {
  console.error("Usage: node patch-qs-diagnostic.js <parse.js>");
  process.exit(1);
}

const source = fs.readFileSync(file, "utf8");
const nodeExpectation =
  "            /^TypeError: Cannot assign to read only property 'frozenProp' of (?:object '#<Object>'|#<Object>)/,";
const conditionalExpectation = [
  "            typeof __host_write === 'undefined'",
  "                ? /^TypeError: Cannot assign to read only property 'frozenProp' of (?:object '#<Object>'|#<Object>)/",
  "                : /^TypeError: Cannot assign to read only property 'frozenProp'$/,",
].join("\n");

if (!source.includes(nodeExpectation)) {
  console.error("qs frozen-property diagnostic assertion not found");
  process.exit(1);
}

fs.writeFileSync(file, source.replace(nodeExpectation, conditionalExpectation));
console.log("qs frozen-property diagnostic assertion patched for JSSE");
