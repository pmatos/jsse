// RUN: 0
// PIN: stderr
// The main module is synchronous but waits for a dependency's top-level await:
// its throw still reaches the handler four microtask turns later.
import "./esm-tla.helper.mjs";
let chain = Promise.resolve();
for (let i = 1; i <= 8; i++) chain = chain.then(() => console.error("tick " + i));
process.on("uncaughtException", (error, origin) => console.error("handler", origin, error.message));
throw new Error("main");
