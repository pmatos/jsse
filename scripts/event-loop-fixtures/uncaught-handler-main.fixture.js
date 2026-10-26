// RUN: 0
// A handled throw from the main script does not stop the program: the tick
// queue, timers and the exit event still run.
process.on("uncaughtException", function (err, origin) {
  console.log("caught", err.message, origin);
});
process.on("exit", function (code) { console.log("exit", code); });
process.nextTick(function () { console.log("tick"); });
setTimeout(function () { console.log("timer"); }, 1);
throw new Error("main");
