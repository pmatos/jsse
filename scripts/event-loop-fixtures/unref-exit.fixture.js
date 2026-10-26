// RUN: 0
// A loop holding only unref'd handles exits at once, firing none of them.
setTimeout(function () { console.log("unrefd-timeout-never"); }, 20).unref();
setInterval(function () { console.log("unrefd-interval-never"); }, 5).unref();
setImmediate(function () { console.log("unrefd-immediate-never"); }).unref();
console.log("main-done");
process.on("exit", function (code) { console.log("exit " + code); });
