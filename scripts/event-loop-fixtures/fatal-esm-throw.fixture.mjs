// RUN: 1
// An uncaught throw at the top level of an ES module is an entry-point
// rejection, so the report points at the error's own top frame (the `new`).
throw new Error("boom");
