// RUN: 1
// An object with message/stack-like keys is not an error.
throw { message: "m", stack: "s" };
