// Tests dense ordinary-Array fast paths for indexed writes and Array.prototype.push.
// Spec: ECMAScript 2024, sec-array-exotic-objects-defineownproperty-p-desc,
//       sec-ordinaryset, sec-array.prototype.push
// These fast paths (issue #125) must be observationally identical to the slow
// path: in-bounds overwrite, append-at-end length bump, hole creation, and
// throw-on-frozen-length in strict mode.

// In-bounds overwrite — value replaced, length unchanged.
var a = [10, 20, 30];
a[1] = 99;
if (a[1] !== 99 || a.length !== 3) {
  throw new Test262Error('in-bounds overwrite: expected a[1]===99 and length 3, got a[1]=' + a[1] + ' length=' + a.length);
}

// In-bounds overwrite via compound assignment.
var c = [5, 6, 7];
c[2] += 10;
if (c[2] !== 17 || c.length !== 3) {
  throw new Test262Error('compound in-bounds overwrite: expected c[2]===17 and length 3, got c[2]=' + c[2] + ' length=' + c.length);
}

// Append at end — element added and length bumped to idx+1.
var b = [1, 2];
b[2] = 3;
if (b[2] !== 3 || b.length !== 3) {
  throw new Test262Error('append at end: expected b[2]===3 and length 3, got b[2]=' + b[2] + ' length=' + b.length);
}

// Append at end via compound assignment.
var d = [1];
d[1] = (d[1] || 0) + 42;
if (d[1] !== 42 || d.length !== 2) {
  throw new Test262Error('compound append: expected d[1]===42 and length 2, got d[1]=' + d[1] + ' length=' + d.length);
}

// Hole creation via arr[len+5] = v — a gap is created and length jumps.
var e = [1, 2];
e[7] = 9;
if (e.length !== 8) {
  throw new Test262Error('hole creation: expected length 8, got ' + e.length);
}
if (e[7] !== 9) {
  throw new Test262Error('hole creation: expected e[7]===9, got ' + e[7]);
}
if (3 in e) {
  throw new Test262Error('hole creation: expected index 3 to be a hole (not present)');
}

// Array.prototype.push appends and returns the new length.
var p = [1, 2, 3];
var n = p.push(4, 5);
if (n !== 5 || p.length !== 5 || p[3] !== 4 || p[4] !== 5) {
  throw new Test262Error('push: expected length 5 with p[3]===4, p[4]===5, got length=' + p.length + ' return=' + n);
}

// Frozen-length append throws TypeError in strict mode (slow path preserved).
(function() {
  "use strict";
  var f = [1, 2];
  Object.defineProperty(f, 'length', { writable: false });
  var threw = false;
  try {
    f[2] = 3;
  } catch (err) {
    threw = err instanceof TypeError;
  }
  if (!threw) {
    throw new Test262Error('frozen-length append: expected TypeError in strict mode');
  }
  if (f.length !== 2 || (2 in f)) {
    throw new Test262Error('frozen-length append: array must be unchanged, got length=' + f.length);
  }
})();

// push on a non-writable-length array throws TypeError (slow path preserved).
var g = [1, 2];
Object.defineProperty(g, 'length', { writable: false });
var pushThrew = false;
try {
  g.push(3);
} catch (err) {
  pushThrew = err instanceof TypeError;
}
if (!pushThrew) {
  throw new Test262Error('push on non-writable length: expected TypeError');
}
