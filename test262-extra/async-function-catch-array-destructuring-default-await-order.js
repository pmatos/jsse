/*---
description: >
  An `await` inside a destructuring default in an *array* catch-parameter
  pattern suspends the async function at a real Await state -- the caller
  continues synchronously and jobs already queued run before the function's
  continuation -- exactly like `await` in an object catch-parameter pattern
  default (issue #773), or a plain declaration's default (issue #725).
esid: sec-runtime-semantics-catchclauseevaluation
info: |
  CatchClauseEvaluation

  ...
  4. Let status be Completion(BindingInitialization of CatchParameter with
     arguments thrownValue and catchEnv).
  ...

  Runtime Semantics: IteratorBindingInitialization
  ArrayBindingPattern : [ Elision_opt BindingRestElement_opt ]
  BindingElement : SingleNameBinding
  SingleNameBinding : BindingIdentifier Initializer_opt

  ...
  If iteratorRecord.[[Done]] is false, ...
  Let v be ? IteratorValue(next).
  ...
  3. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
  ...

  Await ( value )

  ...
  9. Perform PerformPromiseThen(promise, onFulfilled, onRejected).
  10. Remove asyncContext from the execution context stack ...
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

async function viaCatch(L) {
  try {
    throw [];
  } catch ([a = await 5]) {
    L('c' + a);
  }
}
async function viaAsyncGenerator(L) {
  async function* g() {
    try {
      throw [];
    } catch ([a = await 5]) {
      L('c' + a);
    }
  }
  var r = g().next();
  L('called-next');
  await r;
}

var expected = ['sync-end', 'w1', 'c5', 'w2', 'w3'];

Promise.all([run(viaCatch), run(viaAsyncGenerator)])
  .then(function (logs) {
    assert.compareArray(logs[0], expected, 'async function array catch param');
    assert.compareArray(
      logs[1],
      ['called-next', 'sync-end', 'w1', 'c5', 'w2', 'w3'],
      'async generator array catch param'
    );
  })
  .then($DONE, $DONE);
