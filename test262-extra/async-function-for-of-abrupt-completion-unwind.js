/*---
description: >
  Async for-of unwinding carries abrupt completions through nested iteration
  disposal and iterator closing.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  When a for-of body completes abruptly, ForIn/OfBodyEvaluation first disposes
  the current iteration environment and then performs IteratorClose. Each
  enclosing loop receives that resulting completion as its own body result.

  DisposeResources combines a new disposal error with an existing throw in a
  SuppressedError whose error is the new error and whose suppressed value is
  the prior abrupt completion.
flags: [async]
features: [async-functions, explicit-resource-management]
---*/

function throwingResource(name) {
  return {
    [Symbol.dispose]: function () {
      throw name;
    }
  };
}

async function nestedDisposalErrors() {
  for (using outer of [throwingResource('outer')]) {
    for (using inner of [throwingResource('inner')]) {
      await null;
      return 'unreachable';
    }
  }
}

nestedDisposalErrors()
  .then(
    function () {
      throw new Test262Error('nested disposal errors must reject');
    },
    function (error) {
      assert(error instanceof SuppressedError, 'outer disposal creates a SuppressedError');
      assert.sameValue(error.error, 'outer', 'outer disposal error is the primary error');
      assert.sameValue(error.suppressed, 'inner', 'inner disposal error is suppressed');
    }
  )
  .then($DONE, $DONE);
