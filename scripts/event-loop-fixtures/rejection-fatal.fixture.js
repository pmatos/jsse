// RUN: 1
// ERR: rejected-boom
// Without a handler an unhandled rejection is an uncaught exception.
process.on("exit", function (code) { console.log("exit " + code); });
setTimeout(function () { console.log("never"); }, 20);
Promise.reject(new Error("rejected-boom"));
