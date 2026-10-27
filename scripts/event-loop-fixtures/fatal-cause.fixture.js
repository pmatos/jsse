// RUN: 1
// An error with a cause prints the [cause] block.
const inner = new RangeError("inner problem");
throw new Error("outer problem", { cause: inner });
