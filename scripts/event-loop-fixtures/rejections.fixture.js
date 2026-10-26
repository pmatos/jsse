// RUN: 0
// unhandledRejection fires after the tick and microtask drain, once per
// promise still unhandled; a handler attached in time suppresses it.
var log = [];
process.on("unhandledRejection", function (reason, promise) {
  log.push("unhandled " + reason + " " + (promise instanceof Promise));
});
Promise.reject("late-handled").catch(function () { log.push("caught early"); });
var p = Promise.reject("handled-in-tick");
process.nextTick(function () { p.catch(function () { log.push("caught in tick"); }); });
Promise.reject("really-unhandled");
(async function () { throw "async-unhandled"; })();
new Promise(function (_, reject) { setTimeout(reject, 1, "timer-rejection"); });
process.on("exit", function () { console.log(log.join("\n")); });
