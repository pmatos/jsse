/*---
description: >
  A let/const declaration destructuring the result of `yield *` in an async
  generator initializes the bound names, even when the delegated generator
  finishes at the very first step, instead of throwing a TDZ ReferenceError.
esid: sec-let-and-const-declarations-runtime-semantics-evaluation
info: |
  LexicalBinding : BindingPattern Initializer

  1. Let rhs be ? Evaluation of Initializer.
  2. Let value be ? GetValue(rhs).
  3. Return ? BindingInitialization of BindingPattern with arguments value and environment.

  BindingInitialization for a BindingPattern is performed with the running
  execution context's LexicalEnvironment as `environment`, so it goes through
  InitializeReferencedBinding rather than PutValue -- regardless of the fact
  that the initializer's value came from a `yield *` expression.
flags: [async]
features: [async-iteration, destructuring-binding]
---*/

async function constObjectPattern() {
  async function* inner() {
    return { x: 1 };
  }
  async function* g() {
    const { x } = yield* inner();
    yield x;
  }
  var it = g();
  var r = await it.next();
  return r.value;
}

async function letObjectPattern() {
  async function* inner() {
    return { x: 2 };
  }
  async function* g() {
    let { x } = yield* inner();
    yield x;
  }
  var it = g();
  var r = await it.next();
  return r.value;
}

async function constArrayPattern() {
  async function* inner() {
    return [3];
  }
  async function* g() {
    const [x] = yield* inner();
    yield x;
  }
  var it = g();
  var r = await it.next();
  return r.value;
}

Promise.all([constObjectPattern(), letObjectPattern(), constArrayPattern()])
  .then(function (results) {
    assert.sameValue(
      results[0],
      1,
      'const object pattern destructuring a yield* result initializes the binding'
    );
    assert.sameValue(
      results[1],
      2,
      'let object pattern destructuring a yield* result initializes the binding'
    );
    assert.sameValue(
      results[2],
      3,
      'const array pattern destructuring a yield* result initializes the binding'
    );
  })
  .then($DONE, $DONE);
