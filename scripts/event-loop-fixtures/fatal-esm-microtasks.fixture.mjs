// RUN: 1
// Node reports the failure of the entry point four microtask turns after it
// threw: m1..m4 run first, then exit, then the report. Neither the tick queue
// nor the immediate queue runs.
let chain = Promise.resolve();
for (let i = 1; i <= 8; i++) chain = chain.then(() => console.error("m" + i));
process.nextTick(() => console.error("tick"));
setImmediate(() => console.error("immediate"));
process.on("exit", () => console.error("exit"));
throw new Error("microtasks");
