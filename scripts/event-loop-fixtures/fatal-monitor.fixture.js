// RUN: 1
// uncaughtExceptionMonitor sees the error and does not prevent the crash.
process.on("uncaughtExceptionMonitor", (err, origin) => {
  process.stderr.write("monitor " + err.message + " " + origin + "\n");
});
throw new Error("monitored");
