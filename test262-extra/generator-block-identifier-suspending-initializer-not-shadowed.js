/*---
description: >
  A block's own BlockDeclarationInstantiation must not shadow a plain
  identifier `let`/`const` binding whose initializer itself contains the
  suspension point (`const x = yield`/`const x = await p`). The engine
  lowers that specific shape to a function-level temp variable, assigned
  once the suspension resumes, rather than a normal declaration inside the
  block's own Environment Record -- so a fix that eagerly pre-declares the
  block's lexical names as TDZ must recognize this declarator and leave it
  out, or it would shadow the temp variable with a binding that never gets
  initialized.
esid: sec-let-and-const-declarations-runtime-semantics-evaluation
info: |
  LexicalBinding : BindingIdentifier Initializer

  1. Let bindingId be StringValue of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(bindingId).
  3. Let rhs be ? Evaluation of Initializer.
  4. Let value be ? GetValue(rhs).
  ...
  6. Return ? InitializeReferencedBinding(lhs, value).

  Evaluating the Initializer (here, a `yield`/`await`) can suspend
  mid-declaration. The name must still resolve to its own (eventually
  initialized) value once the declaration completes, not to a stale
  uninitialized binding left behind by the block's own
  BlockDeclarationInstantiation.
flags: [async]
features: [generators, async-functions, async-generators]
---*/

function* syncGen() {
  {
    const result = yield;
    return result;
  }
}
var it1 = syncGen();
it1.next();
assert.sameValue(
  it1.next(42).value,
  42,
  'sync generator: a plain identifier declared with a suspending initializer keeps its resumed value'
);

async function asyncFn() {
  {
    const result = await 42;
    return result;
  }
}

async function asyncGenDriver() {
  async function* g() {
    {
      const result = yield;
      return result;
    }
  }
  var it2 = g();
  await it2.next();
  var r = await it2.next(42);
  return r.value;
}

asyncFn()
  .then(function (result) {
    assert.sameValue(
      result,
      42,
      'async function: a plain identifier declared with a suspending initializer keeps its resumed value'
    );
    return asyncGenDriver();
  })
  .then(function (result) {
    assert.sameValue(
      result,
      42,
      'async generator: a plain identifier declared with a suspending initializer keeps its resumed value'
    );
  })
  .then($DONE, $DONE);
