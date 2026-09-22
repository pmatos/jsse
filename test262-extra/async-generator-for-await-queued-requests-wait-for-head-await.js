// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgenerator-prototype-return
description: >
  A return() or throw() request queued while an async generator is suspended at
  a `for await` head's Await is not delivered until the in-flight next() request
  completes at the body's yield.
info: |
  AsyncGenerator.prototype.return ( value )

  6. Perform AsyncGeneratorEnqueue(generator, completion, promiseCapability).
  7. If state is either suspended-start or completed, then
     [...]
  8. Else if state is suspended-yield, perform AsyncGeneratorResume(generator, completion).
  9. Else, Assert: state is either executing or awaiting-return.

  A generator suspended at an Await is in the executing state, so the request
  only waits in the queue; it resumes the generator once the current request has
  been settled by a yield.
includes: [asyncHelpers.js]
flags: [async]
features: [async-iteration]
---*/

asyncTest(async function () {
  var log = [];

  function iterable() {
    var n = 0;
    return {
      [Symbol.asyncIterator]() {
        return {
          next() {
            log.push('next' + (n + 1));
            return Promise.resolve().then(function () {
              return { done: false, value: ++n };
            });
          },
          return() {
            log.push('iter-return');
            return Promise.resolve({});
          }
        };
      }
    };
  }

  async function* g() {
    for await (var x of iterable()) {
      log.push('body' + x);
      yield x;
    }
  }

  var it = g();
  var first = it.next();
  var returned = it.return('R');
  assert.sameValue(log.join(), 'next1', 'the head is suspended at its Await');
  var firstResult = await first;
  assert.sameValue(firstResult.value, 1, 'next() settles at the body yield');
  assert.sameValue(firstResult.done, false, 'next() is not done');
  var returnResult = await returned;
  assert.sameValue(returnResult.value, 'R', 'the queued return() completes with its value');
  assert.sameValue(returnResult.done, true, 'the queued return() finishes the generator');
  assert.sameValue(
    log.join(),
    'next1,body1,iter-return',
    'the body ran before the queued return() closed the iterator'
  );

  log = [];
  it = g();
  first = it.next();
  var thrown = it.throw('E').then(
    function () { return 'resolved'; },
    function (e) { return 'rejected ' + e; }
  );
  firstResult = await first;
  assert.sameValue(firstResult.value, 1, 'next() settles at the body yield before throw() is delivered');
  assert.sameValue(await thrown, 'rejected E', 'the queued throw() rejects with its reason');
  assert.sameValue(
    log.join(),
    'next1,body1,iter-return',
    'the body ran before the queued throw() closed the iterator'
  );
});
