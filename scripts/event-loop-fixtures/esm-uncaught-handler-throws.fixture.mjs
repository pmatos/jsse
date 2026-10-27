// RUN: 7
// A handler that throws ends the process with 7 and the second error is reported.
process.on("uncaughtException", function () { throw new Error("handler failed"); });
throw new Error("first");
