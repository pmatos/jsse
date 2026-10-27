// RUN: 5
// An exit listener sees code 1 and may change the status.
process.on("exit", (code) => {
  console.error("exit", code);
  process.exitCode = 5;
});
throw new Error("exit code");
