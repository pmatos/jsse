// RUN: 1
// ERR: boom
// An uncaught throw in a timer is fatal: exit is emitted with 1, no other
// timer runs, and the status is 1.
process.on("exit", function (code) { console.log("exit " + code + " " + process.exitCode); });
setTimeout(function () { console.log("never"); }, 50);
setTimeout(function () { throw new Error("boom"); }, 1);
