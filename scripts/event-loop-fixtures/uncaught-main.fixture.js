// RUN: 1
// ERR: main-boom
// A throwing main script stops the program before any timer runs.
process.on("exit", function (code) { console.log("exit " + code); });
setTimeout(function () { console.log("never"); }, 0);
setImmediate(function () { console.log("never"); });
process.nextTick(function () { console.log("never-tick"); });
throw new Error("main-boom");
