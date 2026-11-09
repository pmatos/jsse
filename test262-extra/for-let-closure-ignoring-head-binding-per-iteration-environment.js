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
flags: [async]
features: [class-static-block, generators, async-functions]
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

fs = [];
for (let i = 0; i < 3; fs.push(() => i), i++) {}
assert.compareArray(results(fs), [1, 2, 3], "head binding through a closure in the update clause");

fs = [];
for (let i = 0; (fs.push(() => i), i < 3); i++) {}
assert.compareArray(results(fs), [0, 1, 2, 3], "head binding through a closure in the test clause");

fs = [];
for (let i = 0; i < 3; i++) {
  fs.push(eval("() => i"));
}
assert.compareArray(results(fs), [0, 1, 2], "head binding through a direct eval outside any closure");

fs = (0, eval)("var r = []; for (let i = 0; i < 3; i++) { with ({}) { r.push(() => i); } } r");
assert.compareArray(results(fs), [0, 1, 2], "head binding through a closure inside with");

function* generatorLoop() {
  var captured = [];
  for (let i = 0; i < 3; i++) {
    const x = i * 10;
    captured.push(() => x + 1);
    captured.push(() => () => i);
    yield i;
  }
  return captured;
}
var it = generatorLoop();
var step;
while (!(step = it.next()).done) {}
assert.compareArray(
  step.value.map(function (f, k) { return k % 2 ? f()() : f(); }),
  [1, 0, 11, 1, 21, 2],
  "generator loop: body binding and head binding both fresh per iteration"
);

async function asyncLoop() {
  var captured = [];
  for (let i = 0; i < 3; i++) {
    const x = i * 10;
    captured.push(() => x + 1);
    captured.push(() => () => i);
    await 0;
  }
  return captured.map(function (f, k) { return k % 2 ? f()() : f(); });
}
asyncLoop().then(function (values) {
  assert.compareArray(values, [1, 0, 11, 1, 21, 2], "async loop: body binding and head binding both fresh per iteration");
}).then($DONE, $DONE);
