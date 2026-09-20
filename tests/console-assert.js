// Host `console.assert` (WHATWG Console Standard): a falsy condition reports
// "Assertion failed" on stderr and never throws; a truthy one is silent.
// JetStream's validatorjs workload validates its assertion count with it.

function sameValue(actual, expected, label) {
  if (!Object.is(actual, expected)) {
    throw new Error(label + ": expected " + String(expected) +
      ", got " + String(actual));
  }
}

sameValue(typeof console.assert, "function", "typeof console.assert");
sameValue(console.assert.length, 0, "console.assert.length");
sameValue(console.assert(true), undefined, "truthy condition");
sameValue(console.assert(false), undefined, "falsy condition does not throw");
sameValue(console.assert(false, "detail", 1), undefined, "falsy condition with data");
sameValue(console.assert(), undefined, "missing condition is falsy");
sameValue(console.assert(0, { toString() { return "obj"; } }), undefined, "object data");
