// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorawaitreturn
description: >
  While AsyncGeneratorAwaitReturn waits for its operand, the generator, the
  request being served and the requests queued behind it stay reachable across
  a garbage collection even though nothing else references them.
info: |
  AsyncGeneratorAwaitReturn ( generator )

  The fulfilledClosure and rejectedClosure capture generator and complete its
  first queued request, then drain the rest of the queue, so all of it must
  stay live until the operand settles.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, host-gc-required]
---*/

function start(settle) {
  return new Promise(function (done) {
    var release;
    var operand = new Promise(function (resolve, reject) {
      release = settle === 'reject' ? reject : resolve;
    });
    var log = [];
    (function () {
      var it = (async function* () {})();
      it.return(operand).then(
        function (r) { log.push('return:' + r.value + ':' + r.done); },
        function (e) { log.push('return-rejected:' + e); }
      );
      it.next().then(function (r) { log.push('next:' + r.done); });
      it.throw('T').then(function () {}, function (e) { log.push('throw:' + e); });
    })();
    operand = null;
    setTimeout(function () {
      $262.gc();
      setTimeout(function () {
        $262.gc();
        release('V');
        setTimeout(function () { done(log); }, 0);
      }, 0);
    }, 0);
  });
}

asyncTest(async function () {
  assert.compareArray(
    await start('fulfill'),
    ['return:V:true', 'next:true', 'throw:T'],
    'fulfilled operand: every queued request settles in order after gc'
  );
  assert.compareArray(
    await start('reject'),
    ['return-rejected:V', 'next:true', 'throw:T'],
    'rejected operand: every queued request settles in order after gc'
  );
});
