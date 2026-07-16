// RegExp multiline anchors must recognize every ECMAScript LineTerminator.
// Spec: ECMAScript 2026, sec-assertion (CompileAssertion for ^ and $)

var terminators = ["\n", "\r", "\u2028", "\u2029"];

for (var i = 0; i < terminators.length; i++) {
  var terminator = terminators[i];
  var endMatch = /x$/m.exec("x" + terminator);
  if (endMatch === null || endMatch.index !== 0 || endMatch[0] !== "x") {
    throw new Test262Error("$ did not match before line terminator " + i);
  }

  var startMatch = /^x/m.exec(terminator + "x");
  if (startMatch === null || startMatch.index !== 1 || startMatch[0] !== "x") {
    throw new Test262Error("^ did not match after line terminator " + i);
  }
}

if (/x$/.test("x\n")) {
  throw new Test262Error("$ without multiline matched before a line terminator");
}
if (/^x/.test("\nx")) {
  throw new Test262Error("^ without multiline matched after a line terminator");
}
