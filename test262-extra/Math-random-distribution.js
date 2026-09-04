// Math.random must return approximately uniformly distributed Numbers in
// [0, 1), and successive calls must form a pseudo-random sequence rather than
// a constant value.
// Spec: ECMAScript, sec-math.random.

var first = Math.random();
var varied = false;

for (var i = 0; i < 100; i++) {
  var value = Math.random();
  if (typeof value !== "number" || value < 0 || value >= 1) {
    throw new Test262Error("Math.random returned an invalid value: " + value);
  }
  if (value === 0 && 1 / value < 0) {
    throw new Test262Error("Math.random returned negative zero");
  }
  if (value !== first) {
    varied = true;
  }
}

if (!varied) {
  throw new Test262Error("Math.random returned a constant sequence");
}

var randomA = $262.createRealm().global.Math.random;
var randomB = $262.createRealm().global.Math.random;
var realmSequencesDiffer = false;

for (var j = 0; j < 16; j++) {
  if (randomA() !== randomB()) {
    realmSequencesDiffer = true;
  }
}

if (!realmSequencesDiffer) {
  throw new Test262Error("distinct realms produced the same random sequence");
}
