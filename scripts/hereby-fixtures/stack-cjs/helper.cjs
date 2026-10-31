function fail() {
  throw new Error("cjs helper failed");
}
exports.run = function () { fail(); };
exports.run();
