// RUN: 1
// A rethrow from a catch block reports the rethrow site.
try {
  throw new Error("first");
} catch (e) {
  throw e;
}
