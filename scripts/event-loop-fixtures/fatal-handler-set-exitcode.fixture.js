// RUN: 5
// An exit listener may change the status of a fatal exception.
process.on("exit", () => { process.exitCode = 5; });
throw new Error("changed by listener");
