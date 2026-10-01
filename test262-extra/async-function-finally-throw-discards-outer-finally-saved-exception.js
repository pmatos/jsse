/*---
description: >
  A throw from a finally body that is itself handling an exception replaces
  that exception outright. If an enclosing catch then completes normally, the
  replaced exception must not resurface afterward.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  TryStatement : try Block Finally
  3. Let F be Completion(Evaluation of Finally).
  4. If F.[[Type]] is normal, set F to B.
  5. Return ? UpdateEmpty(F, undefined).
  When Finally's own evaluation is abrupt (here, a throw raised while the
  async function's await of the `try` block's own throw is suspended), F
  keeps that abrupt completion and B (the exception the Finally was invoked
  to finish) is discarded, never observed again once a later handler
  completes normally.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

function outerCatchDoesNotSeeDiscardedException() {
  var log = [];
  async function f() {
    try {
      try {
        await 1;
        throw 'inner-throw';
      } finally {
        throw 'finally-throw';
      }
    } catch (e) {
      log.push('caught:' + e);
    }
    return 'done';
  }
  return f().then(
    function (v) {
      log.push('resolved:' + v);
      return log;
    },
    function (e) {
      log.push('rejected:' + e);
      return log;
    }
  );
}

outerCatchDoesNotSeeDiscardedException().then(function (log) {
  assert.compareArray(
    log,
    ['caught:finally-throw', 'resolved:done'],
    'the exception the inner finally replaced must not resurface after the outer catch completes normally'
  );
}).then($DONE, $DONE);
