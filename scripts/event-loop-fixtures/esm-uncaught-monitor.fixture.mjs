// RUN: 0
// PIN: stderr
process.on("uncaughtExceptionMonitor", (error, origin) => console.error("monitor", origin, error));
process.on("uncaughtException", (error, origin) => console.error("uncaught", origin, error));
await null;
throw 7;
