// Stack frames of code loaded through require() (issue #1228): the file name,
// line and column of an error created in a required module must match Node's,
// including a module with a leading shebang line. Run verbatim on Node and on
// `jsse --node` by scripts/run-node-v8-error-oracle.sh.
const thrower = require("./lib/thrower");
const second = require("./lib/second");

function frames(error, from, to) {
  return error.stack.split("\n").slice(from, to).map((line) => line.trim()).join("\n");
}

function show(label, value) {
  console.log("## " + label);
  console.log(value);
}

function caught() {
  try {
    thrower.fail();
  } catch (error) {
    return error;
  }
}

function main() {
  show("created in a module function", frames(thrower.inner(), 1, 3));
  show("created at module top level", frames(thrower.topLevel, 1, 2));
  show("through two modules", frames(second.run(), 1, 4));
  show("thrown from a module", frames(caught(), 0, 3));
  show("main module", frames(new Error("main"), 1, 2));
  show("wrapper name", [typeof require, __filename.endsWith("modules.js")].join());
}
main();
