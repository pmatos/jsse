// RUN: 1
// An AggregateError prints its [errors] block.
throw new AggregateError([new Error("a"), new TypeError("b")], "many");
