// RUN: 1
// A module that has a top-level await is reported one turn later even when it
// throws before reaching it.
let chain = Promise.resolve();
for (let i = 1; i <= 8; i++) chain = chain.then(() => console.error("tick " + i));
process.on("exit", () => console.error("exit"));
if (chain) throw new Error("early");
await null;
