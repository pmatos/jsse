// RUN: 0
// With only an uncaughtException handler, a rejection arrives there with
// origin "unhandledRejection" (a non-error reason is wrapped in an Error).
process.on("uncaughtException", function (err, origin) {
  console.log(origin, err instanceof Error, err.code, err.message.slice(0, 40));
});
Promise.reject(42);
