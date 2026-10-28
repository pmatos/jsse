// RUN: 0
// PIN: stderr
// An entry-point failure reaches uncaughtException with origin
// unhandledRejection and never emits the unhandledRejection event.
process.on("unhandledRejection", () => console.error("unhandledRejection event"));
process.on("uncaughtException", (error, origin) => console.error("uncaught", origin, error.message));
process.on("exit", (code) => console.error("exit", code));
throw new Error("handled");
