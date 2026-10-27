// RUN: 1
// A throw after a top-level await.
await null;
console.error("resumed");
throw new RangeError("after await");
