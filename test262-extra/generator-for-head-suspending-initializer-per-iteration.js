/*---
description: >
  A `for` loop's lexical head whose own initializer contains a suspension
  point (`for (let i = yield; ...)`/`for (let i = await p; ...)`) must
  still get correct `CreatePerIterationEnvironment` semantics: the engine
  lowers the head's plain-identifier initializer to a function-level temp
  variable (the same shape as the block case), so the per-iteration
  environment's first copy of the name must read that temp variable's
  resumed value rather than an uninitialized binding a pre-declare step
  left shadowing it.
esid: sec-createperiterationenvironment
info: |
  CreatePerIterationEnvironment ( perIterationBindings )

  ...
  For each element bn of perIterationBindings, do
    a. Perform ! thisIterationEnv.CreateMutableBinding(bn, false).
    b. Let lastValue be ? lastIterationEnv.GetBindingValue(bn, true).
    c. Perform thisIterationEnv.InitializeBinding(bn, lastValue).

  The very first per-iteration environment's `lastValue` read must observe
  the lexical head's own (possibly suspended) initializer having already
  run, wherever its value actually lives.
flags: [async]
includes: [compareArray.js]
features: [generators, async-functions]
---*/

function* syncGen() {
  var arr = [];
  for (let i = yield; i < 2; i++) {
    arr.push(i);
  }
  return arr;
}
var it = syncGen();
it.next();
assert.compareArray(
  it.next(0).value,
  [0, 1],
  'sync generator: a for-head identifier with a suspending initializer drives correct iteration'
);

async function asyncFn() {
  async function f(n) {
    return n;
  }
  var closures = [];
  for (let i = await f(0); i < 3; i++) {
    closures.push(function () { return i; });
  }
  return closures.map(function (fn) { return fn(); });
}

asyncFn().then(function (values) {
  assert.compareArray(
    values,
    [0, 1, 2],
    'async function: a for-head identifier with a suspending initializer still gets distinct per-iteration bindings'
  );
}).then($DONE, $DONE);
