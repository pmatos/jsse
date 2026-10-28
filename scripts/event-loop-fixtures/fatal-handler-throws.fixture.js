// RUN: 7
// A throwing uncaughtException handler is fatal with status 7.
process.on("uncaughtException", function () {
  throw new Error("handler failed");
});
throw new Error("original");
