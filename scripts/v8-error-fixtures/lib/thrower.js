#!/usr/bin/env node
function inner() { return new Error("in module"); }
function fail() {
  throw new TypeError("thrown in module");
}
const topLevel = new Error("top level");
exports.inner = inner;
exports.fail = fail;
exports.topLevel = topLevel;
