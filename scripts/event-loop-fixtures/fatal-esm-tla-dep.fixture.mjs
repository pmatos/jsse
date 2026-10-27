// RUN: 1
// The dependency's frame is gone once it has finished: the main module's
// stack has only its own top-level frame.
import "./esm-tla.helper.mjs";
let chain = Promise.resolve();
for (let i = 1; i <= 8; i++) chain = chain.then(() => console.error("tick " + i));
process.on("exit", () => console.error("exit"));
throw new Error("main");
