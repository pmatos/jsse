/*---
description: >
  An await inside an array-pattern element's default suspends the async
  function at a real Await state: the caller continues synchronously and
  jobs already queued run before the function's continuation, exactly as
  for an await in an object-pattern default (see issue #725, a follow-up
  of #709 which only lowered object patterns). Also covers an array
  pattern nested inside an object pattern's property, and the equivalent
  case in an async generator.
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  IteratorBindingInitialization

  SingleNameBinding : BindingIdentifier Initializer_opt
  ...
  3. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
  ...
flags: [async]
includes: [compareArray.js]
features: [async-functions, async-iteration, destructuring-binding]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
  var p = makeFn(L);
  L('sync-end');
  return p.then(function () { return log; });
}

async function viaVar(L) { var [a = await 1] = []; L('a' + a); }
async function viaLet(L) { let [a = await 1] = []; L('a' + a); }
async function viaConst(L) { const [a = await 1] = []; L('a' + a); }
var viaArrow = async (L) => { var [a = await 1] = []; L('a' + a); };
async function viaNestedInObject(L) { var { x: [a = await 1] } = { x: [] }; L('a' + a); }
async function viaAsyncGenerator(L) {
  async function* g() { var [a = await 1] = []; L('a' + a); }
  var r = g().next();
  L('called-next');
  await r;
}

var expected = ['sync-end', 'w1', 'a1', 'w2', 'w3'];

Promise.all([
  run(viaVar),
  run(viaLet),
  run(viaConst),
  run(viaArrow),
  run(viaNestedInObject),
  run(viaAsyncGenerator),
]).then(function (logs) {
  assert.compareArray(logs[0], expected, 'var');
  assert.compareArray(logs[1], expected, 'let');
  assert.compareArray(logs[2], expected, 'const');
  assert.compareArray(logs[3], expected, 'async arrow');
  assert.compareArray(logs[4], expected, 'array pattern nested inside an object pattern');
  assert.compareArray(
    logs[5],
    ['called-next', 'sync-end', 'w1', 'a1', 'w2', 'w3'],
    'async generator'
  );
}).then($DONE, $DONE);
