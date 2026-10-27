// RUN: 7
// Node does not emit exit when an uncaughtException handler throws.
process.on("exit", function (code) { console.log("exit", code); });
process.on("uncaughtException", function () {
  throw new Error("handler failed");
});
throw new Error("original");
