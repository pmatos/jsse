// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorenqueue
description: >
  Requests made on a suspended-start or completed async generator settle in
  the order they were made, even though return awaits its operand before
  settling and throw/next settle immediately.
info: |
  %AsyncGeneratorPrototype%.return ( value )

  [...]
  6. If state is either suspended-start or completed, then
     a. Set generator.[[AsyncGeneratorState]] to draining-queue.
     b. Perform AsyncGeneratorAwaitReturn(generator).
  [...]
  8. Return promiseCapability.[[Promise]].

  AsyncGeneratorEnqueue appends the request to the generator's queue, and a
  request is only started while the generator is not executing and not
  draining-queue, so requests made while an AsyncGeneratorAwaitReturn is
  pending wait for it and are served in order by AsyncGeneratorDrainQueue.

  The order in which the three request promises' reactions run is the
  observable.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration]
---*/

async function* g() {}

async function completed() {
  var it = g();
  await it.next();
  return it;
}

function requests(it) {
  var log = [];
  var settled = [
    it.return(1).then(function () { log.push('return'); }),
    it.throw('E').then(function () {}, function () { log.push('throw'); }),
    it.next().then(function () { log.push('next'); })
  ];
  return Promise.all(settled).then(function () { return log; });
}

asyncTest(async function () {
  var log = await requests(g());
  assert.compareArray(log, ['return', 'throw', 'next'], 'from suspended-start');

  log = await requests(await completed());
  assert.compareArray(log, ['return', 'throw', 'next'], 'from completed');
});
