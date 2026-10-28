// RUN: 1
// An error without a stack prints as [Error: message].
const e = new Error("stackless");
Object.defineProperty(e, "stack", { value: undefined });
throw e;
