// RUN: 1
// A stack that is only text prints in brackets.
const e = new Error("x");
e.stack = "custom stack text";
throw e;
