// Array.prototype.push must create *present* properties, including for an
// `undefined` argument. Regression test for the dense-Array push fast path
// (issue #125): in this engine a hole is `Undefined`-in-`Vec` with no
// `properties` entry, so the fast path must not store `undefined` straight
// into the backing Vec without the presence marker the slow path adds —
// otherwise `push(undefined)` would create a hole instead of a present element.
// Spec: ECMAScript 2024, sec-array.prototype.push (step 4.d uses Set, which
// creates an own data property), sec-ordinaryset, sec-hasproperty.

// push(undefined) on an empty array creates a present own property at index 0.
var a = [];
a.push(undefined);
if (a.length !== 1) {
  throw new Test262Error('push(undefined): expected length 1, got ' + a.length);
}
if (!(0 in a)) {
  throw new Test262Error('push(undefined): expected `0 in a` to be true (present, not a hole)');
}
if (!a.hasOwnProperty(0)) {
  throw new Test262Error('push(undefined): expected a.hasOwnProperty(0) to be true');
}
if (a[0] !== undefined) {
  throw new Test262Error('push(undefined): expected a[0] === undefined, got ' + String(a[0]));
}

// Object.keys enumerates present undefined elements.
var b = [];
b.push(undefined, undefined);
var keys = Object.keys(b);
if (keys.length !== 2 || keys[0] !== '0' || keys[1] !== '1') {
  throw new Test262Error('push(undefined,undefined): expected keys ["0","1"], got ' + JSON.stringify(keys));
}

// Mixed present values and undefined: every appended index is present.
var c = [];
c.push(1, undefined, 3);
if (!(0 in c) || !(1 in c) || !(2 in c)) {
  throw new Test262Error('push(1,undefined,3): all of indices 0,1,2 must be present');
}
if (c.length !== 3 || c[0] !== 1 || c[1] !== undefined || c[2] !== 3) {
  throw new Test262Error('push(1,undefined,3): values/length mismatch, length=' + c.length);
}

// Appending undefined onto a populated dense array preserves presence.
var d = [10, 20];
d.push(undefined);
if (d.length !== 3 || !(2 in d) || d[2] !== undefined) {
  throw new Test262Error('push(undefined) onto [10,20]: index 2 must be a present undefined');
}

// forEach must visit a present undefined element (it skips only holes).
var e = [];
e.push(undefined);
var visited = 0;
e.forEach(function () { visited++; });
if (visited !== 1) {
  throw new Test262Error('forEach over push(undefined): expected 1 visit, got ' + visited);
}

// Sanity control: a genuine elision hole is NOT present (the fast path must
// still report real holes correctly).
var h = [, ];
if (0 in h) {
  throw new Test262Error('genuine hole control: expected `0 in h` to be false');
}
