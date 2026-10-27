// RUN: 1
// An uncaught throw in a nested call: the report points at the throw keyword.
function inner() {
  throw new Error("boom");
}
function outer() { inner(); }
outer();
