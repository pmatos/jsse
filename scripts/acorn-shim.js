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
      if (code !== 0) throw new Error("Process exit with code " + code);
    },
    stdout: { write: function(s) {} }
  };
}
