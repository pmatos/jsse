/*---
description: >
  An await in a destructuring default suspends an async generator at its
  Await state, alone and mixed with a yield default in the same pattern.
esid: sec-runtime-semantics-keyedbindinginitialization
info: |
  KeyedBindingInitialization : BindingElement : BindingPattern Initializer_opt

  ...
  3. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
  ...
flags: [async]
includes: [compareArray.js]
features: [async-iteration, destructuring-binding]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return p.then(function () { return log; });
}

async function awaitOnly(L) {
  async function* g() { var { a = await 1 } = {}; L('a' + a); yield a; }
  var it = g();
  L('created');
  var r = it.next();
  L('called-next');
  await r;
  L('end');
}

async function mixed(L) {
  async function* g() { var { a = await 1, b = yield 2 } = {}; L('a' + a + b); return b; }
  var it = g();
  var r1 = await it.next();
  L('y' + r1.value);
  var r2 = await it.next('s');
  L('r' + r2.value + r2.done);
}

Promise.all([run(awaitOnly), run(mixed)]).then(function (logs) {
  assert.compareArray(
    logs[0],
    ['created', 'called-next', 'sync-end', 'w1', 'a1', 'w2', 'w3', 'end'],
    'await default in an async generator'
  );
  assert.compareArray(
    logs[1],
    ['sync-end', 'w1', 'w2', 'w3', 'y2', 'a1s', 'rstrue'],
    'await default then yield default in one pattern'
  );
}).then($DONE, $DONE);
