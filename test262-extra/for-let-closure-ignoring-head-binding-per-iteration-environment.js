/*---
description: >
  A C-style `for` loop with a `let` head may share one head environment across
  iterations when no function, class or eval in the loop can name a head
  binding. Closures that do reach the head binding, by any route, still see a
  fresh binding per iteration, and body bindings are always fresh.
esid: sec-forbodyevaluation
info: |
  ForBodyEvaluation ( test, increment, stmt, perIterationBindings, labelSet )

  2. Perform ? CreatePerIterationEnvironment(perIterationBindings).
  3. Repeat,
    ...
    e. Perform ? CreatePerIterationEnvironment(perIterationBindings).
includes: [compareArray.js]
features: [class-static-block]
---*/

function results(fs) {
  return fs.map(function (f) { return f(); });
}

var fs = [];
for (let i = 0; i < 3; i++) {
  const x = i * 10;
  fs.push(() => x + 1);
}
assert.compareArray(results(fs), [1, 11, 21], "closure over body binding only");

fs = [];
for (let i = 0; i < 3; i++) {
  const x = i;
  fs.push(function () { return x; });
  i += 0;
}
assert.compareArray(results(fs), [0, 1, 2], "body binding fresh while head binding is written");

fs = [];
for (let i = 0; i < 3; i++) {
  fs.push(() => () => i);
}
assert.compareArray(results(fs.map(function (f) { return f(); })), [0, 1, 2], "head binding two functions deep");

fs = [];
for (let i = 0; i < 3; i++) {
  fs.push(() => eval("i"));
}
assert.compareArray(results(fs), [0, 1, 2], "head binding through a direct eval in an arrow");

fs = [];
for (let i = 0; i < 3; i++) {
  fs.push(() => new (class { f = i; })().f);
}
assert.compareArray(results(fs), [0, 1, 2], "head binding through a class field initializer");

fs = [];
for (let i = 0; i < 3; i++) {
  fs.push(() => { var r; new (class { static { r = i; } })(); return r; });
}
assert.compareArray(results(fs), [0, 1, 2], "head binding through a class static block");

fs = [];
for (let i = 0; i < 3; i++) {
  const x = i;
  fs.push((i) => i + x);
}
assert.compareArray(fs.map(function (f) { return f(100); }), [100, 101, 102], "inner parameter shadows the head binding");

fs = [];
for (let i = 0, j = 5; i < 3; i++) {
  fs.push(() => j + i);
  j++;
}
assert.compareArray(results(fs), [6, 8, 10], "closure reaching both head bindings");

fs = [];
for (let i = 0; i < 3; i++) {
  const x = i;
  var g = () => x;
  fs.push(g);
  var h = function () { return typeof i; };
}
assert.compareArray(results(fs), [0, 1, 2], "body binding fresh next to a closure that names the head binding");
assert.sameValue(h(), "number", "a closure that names the head binding still resolves it");
