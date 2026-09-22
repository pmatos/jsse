/*---
description: >
  A return injected by Generator.prototype.return while an outer finally is
  running is owned by that finally's own try context. A nested try/finally
  entered inside the outer finally's body must run to its own completion,
  including any statements after it, before the outer finally's own TryExit
  restores the pending return.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block. If the Finally clause completes normally
  the original completion (here a return) is restored; a nested try/finally
  entered inside the Finally clause has its own, independent Completion
  Record and must not be able to consume or observe the outer one.
includes: [compareArray.js]
features: [generators]
---*/

var log = [];
function* g() {
  try {
    yield 0;
  } finally {
    try {
      yield 1;
    } finally {
      log.push('inner');
    }
    log.push('after');
  }
}

var it = g();
assert.sameValue(it.next().value, 0, 'suspended inside the try block, before any finally runs');
var result = it.return(42);
assert.sameValue(result.value, 1, 'return() enters the outer finally, which yields from the nested try');
assert.sameValue(result.done, false, 'the generator is not done while the nested try is still suspended');

result = it.next();
assert.sameValue(result.value, 42, 'the outer pending return survives the nested try/finally');
assert.sameValue(result.done, true, 'the generator completes with the pending return value');
assert.compareArray(
  log,
  ['inner', 'after'],
  'the nested finally and the statement after it both run before the pending return is restored'
);
