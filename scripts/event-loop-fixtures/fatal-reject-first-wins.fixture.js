// RUN: 1
// Only the first unhandled rejection is reported.
Promise.reject(new Error("first"));
Promise.reject(new Error("second"));
