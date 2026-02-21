// Runtime shims for acorn test runner on jsse.
// Provides Node.js globals that the bundled test runner references.

if (typeof console.group === "undefined") {
  console.group = function(name) { console.log("--- " + name + " ---"); };
}
if (typeof console.groupEnd === "undefined") {
  console.groupEnd = function() {};
}
if (typeof process === "undefined") {
  globalThis.process = {
    exit: function(code) {
      // exit(0) is a no-op — acorn's runner calls it at the end, so execution
      // simply falls through. Non-zero throws to signal failure.
      if (code !== 0) throw new Error("Process exit with code " + code);
    },
    stdout: { write: function(s, cb) { if (typeof cb === "function") cb(); } }
  };
}
