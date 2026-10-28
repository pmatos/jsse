// RUN: 1
// A throw in a timer callback; exit listeners run before the report.
process.on("exit", (code) => { process.stderr.write("exit " + code + "\n"); });
setTimeout(function () {
  throw new Error("late boom");
}, 1);
