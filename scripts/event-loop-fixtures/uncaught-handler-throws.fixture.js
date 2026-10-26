// RUN: 7
// ERR: handler-boom
process.on("uncaughtException", function () { throw new Error("handler-boom"); });
setTimeout(function () { throw new Error("first"); }, 1);
