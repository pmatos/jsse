/*---
description: >
  A transformed for-of at the end of a try clause preserves the enclosing
  loop's continuation state.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try statement preserves the completion of its selected
  block or catch clause unless a finally clause replaces it. A for-of body
  evaluates its statement before continuing the enclosing iteration.

  An implementation that lowers await to a state machine must not let a
  clause's final transformed for-of replace the state supplied by an enclosing
  loop.
flags: [async]
features: [async-functions]
---*/

async function forOfAtEndOfTry() {
  for (const outer of [1]) {
    try {
      for (const inner of [2]) {
        await null;
        return outer + inner;
      }
    } catch (error) {
      return 'caught ' + error;
    }
  }
}

async function forOfAtEndOfCatch() {
  for (const outer of [1]) {
    try {
      await Promise.reject('enter catch');
    } catch (error) {
      for (const inner of [2]) {
        await null;
        return outer + inner;
      }
    }
  }
}

async function forOfAtEndOfFinally() {
  for (const outer of [1]) {
    try {
      await null;
    } finally {
      for (const inner of [2]) {
        await null;
        return outer + inner;
      }
    }
  }
}

forOfAtEndOfTry()
  .then(function (value) {
    assert.sameValue(value, 3, 'the try-final for-of preserves the outer head');
    return forOfAtEndOfCatch();
  })
  .then(function (value) {
    assert.sameValue(value, 3, 'the catch-final for-of preserves the outer head');
    return forOfAtEndOfFinally();
  })
  .then(function (value) {
    assert.sameValue(value, 3, 'the finally-final for-of preserves the outer head');
  })
  .then($DONE, $DONE);
