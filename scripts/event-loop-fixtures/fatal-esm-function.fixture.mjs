// RUN: 1
// The error is built in a function and thrown at the top level: the caret
// follows the error's top frame, not the `throw` keyword.
function make() {
  return new TypeError("made elsewhere");
}
const error = make();
throw error;
