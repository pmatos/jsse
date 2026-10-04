/*---
description: >
  An `await` inside a destructuring default in a C-style `for` statement's
  *array* Variable-kind initializer suspends the async function at a real
  Await state -- the caller continues synchronously and jobs already queued
  run before the function's continuation -- exactly like an object for-init
  pattern default (issue #773), or a plain declaration's default (issue
  #725). Each lexical (`let`) head also gets its own per-iteration
  environment, so a closure created in one iteration keeps that iteration's
  own binding.
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  Runtime Semantics: IteratorBindingInitialization
  ArrayBindingPattern : [ Elision_opt BindingRestElement_opt ]
  SingleNameBinding : BindingIdentifier Initializer_opt

  ...
  3. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
  ...

  Await ( value )

  ...
  9. Perform PerformPromiseThen(promise, onFulfilled, onRejected).
  10. Remove asyncContext from the execution context stack ...

  CreatePerIterationEnvironment ( perIterationBindings )

  ...
  3. If perIterationBindings has any elements, then
    a. Let lastIterationEnv be the running execution context's LexicalEnvironment.
    ...
    f. For each element bn of perIterationBindings, do
      i. Perform ! thisIterationEnv.CreateMutableBinding(bn, false).
      ii. Let lastValue be ? lastIterationEnv.GetBindingValue(bn, true).
      iii. Perform thisIterationEnv.InitializeBinding(bn, lastValue).
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

async function viaVar(L) {
  for (var [a = await 1] = []; ; ) {
    L('a' + a);
    break;
  }
}
async function viaLet(L) {
  for (let [a = await 1] = []; ; ) {
    L('a' + a);
    break;
  }
}
async function viaConst(L) {
  for (const [a = await 1] = []; ; ) {
    L('a' + a);
    break;
  }
}
async function viaAsyncGenerator(L) {
  async function* g() {
    for (var [a = await 1] = []; ; ) {
      L('a' + a);
      break;
    }
  }
  var r = g().next();
  L('called-next');
  await r;
}

var expected = ['sync-end', 'w1', 'a1', 'w2', 'w3'];

Promise.all([run(viaVar), run(viaLet), run(viaConst), run(viaAsyncGenerator)])
  .then(function (logs) {
    assert.compareArray(logs[0], expected, 'var');
    assert.compareArray(logs[1], expected, 'let');
    assert.compareArray(logs[2], expected, 'const');
    assert.compareArray(
      logs[3],
      ['called-next', 'sync-end', 'w1', 'a1', 'w2', 'w3'],
      'async generator'
    );

    return (async function () {
      var fns = [];
      for (let [a = await 0] = []; a < 2; a++) {
        fns.push(function () { return a; });
      }
      assert.sameValue(fns[0](), 0, 'closure from the first iteration keeps its own binding');
      assert.sameValue(fns[1](), 1, 'closure from the second iteration keeps its own binding');
    })();
  })
  .then($DONE, $DONE);
