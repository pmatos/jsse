// RUN: 7
process.on("uncaughtException", function (err, origin) {
  process.stdout.write(origin + "\n");
  throw new Error("handler failed");
});
Promise.reject(new Error("seen"));
