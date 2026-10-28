// RUN: 1
// A fatal rejection exits with 1 even when process.exitCode was set earlier.
process.exitCode = 3;
process.on("exit", (code) => { process.stderr.write("exit " + code + "\n"); });
Promise.reject(new Error("with code"));
