// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-iteratordestructuringassignmentevaluation
description: >
  An `await` inside an array-assignment pattern's element default
  (`[a = await x] = []`) genuinely suspends the enclosing async
  function/async generator at that `Await`, instead of running through the
  tree-walker's blocking fallback. Inside an async generator this matters
  observably: while the generator is parked there, `[[AsyncGeneratorState]]`
  is `executing`, so a concurrent `.return()` request (AsyncGenerator.prototype.return
  step 6) only enqueues -- it never settles until the generator reaches a real
  `suspended-yield` or `completed` state, which an `await` on a
  never-settling promise never reaches.
info: |
  AssignmentElement : DestructuringAssignmentTarget Initializer?

  1. If DestructuringAssignmentTarget is neither an ObjectLiteral nor an
     ArrayLiteral, then
    a. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  2. Let value be undefined.
  3. If iteratorRecord.[[Done]] is false, then
    a. Let next be ? IteratorStepValue(iteratorRecord).
    [...]
  4. If Initializer is present and value is undefined, then
    [...]
    b. Let defaultValue be ? Evaluation of Initializer.
    c. Let v be ? GetValue(defaultValue).
  [...]
  7. Return ? PutValue(lRef, v).

  %AsyncGeneratorPrototype%.return ( value )

  3. Let state be generator.[[AsyncGeneratorState]].
  4. If state is either suspended-start or completed, then [...]
  5. Else if state is suspended-yield, then [...]
  6. Else,
    a. Assert: state is either executing or draining-queue.
  7. Return promiseCapability.[[Promise]].
flags: [async]
includes: [asyncHelpers.js]
features: [async-iteration, destructuring-assignment]
---*/

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 20; i++) p = p.then(function () {});
  return p;
}

asyncTest(async function () {
  // Plain async function: the await never settles, so the function's own
  // returned promise never settles either.
  var a;
  async function f() {
    [a = await new Promise(function () {})] = [];
    return 'unreachable';
  }
  var fSettled = false;
  f().then(
    function () { fSettled = true; },
    function () { fSettled = true; }
  );
  await flush();
  assert.sameValue(fSettled, false, 'a never-settling default await leaves the async function pending');

  // Async generator: a concurrent .return() only enqueues while parked at
  // the Await -- it must not settle either, since the generator never
  // reaches suspended-yield or completed.
  var b;
  async function* g() {
    [b = await new Promise(function () {})] = [];
    yield 'unreachable';
  }
  var gen = g();
  var nextSettled = false;
  var returnSettled = false;
  var nextResult = gen.next();
  nextResult.then(
    function () { nextSettled = true; },
    function () { nextSettled = true; }
  );
  var returnResult = gen.return('done-value');
  returnResult.then(
    function () { returnSettled = true; },
    function () { returnSettled = true; }
  );
  await flush();
  assert.sameValue(nextSettled, false, 'the in-flight next() stays pending behind the stuck Await');
  assert.sameValue(
    returnSettled,
    false,
    '.return() during a pending Await only enqueues -- it must not settle early'
  );
});
