// RUN: 1
// MODE: eval
// A frame per call, then the wrapper; the header is the throw statement.
function inner() {
  throw new TypeError('deep');
}
function outer() {
  inner();
}
outer();
