// RUN: 1
// An unhandled rejection with an Error reports it like an uncaught throw.
process.on("exit", (code) => { process.stderr.write("exit " + code + "\n"); });
Promise.reject(new Error("rejected"));
