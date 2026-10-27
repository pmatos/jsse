// RUN: 1
// After a top-level await the failure takes one more microtask turn.
let chain = Promise.resolve();
for (let i = 1; i <= 12; i++) chain = chain.then(() => console.error("m" + i));
process.on("exit", () => console.error("exit"));
await null;
await null;
console.error("resumed");
throw new Error("microtasks after await");
