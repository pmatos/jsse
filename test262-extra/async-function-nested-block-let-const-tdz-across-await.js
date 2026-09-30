/*---
description: >
  A nested block inside a plain async function that declares a shadowing
  let/const and also contains a suspension point (await) anywhere in the
  block must have every lexical name of that block placed in TDZ, in the
  block's own Environment Record, from block entry -- regardless of how
  many state-machine states the block's StatementList ends up split
  across. A read of the shadowed name that runs before the block's own
  declaration statement executes must reject the async function's promise
  with a TDZ ReferenceError, not fall through the environment chain to an
  outer var/parameter of the same name.
esid: sec-blockdeclarationinstantiation
info: |
  BlockDeclarationInstantiation ( code, env )

  1. Let declarations be the LexicallyScopedDeclarations of code.
  2. For each element d of declarations, do
    a. For each element dn of the BoundNames of d, do
      i. If IsConstantDeclaration of d is true, then
        1. Perform ! env.CreateImmutableBinding(dn, true).
      ii. Else,
        1. Perform ! env.CreateMutableBinding(dn, false).

  Every lexically-scoped declaration of a Block is created, uninitialized,
  in the block's own Environment Record before any statement of the block
  runs. This happens once, for the whole block, independent of whether an
  async function's state-machine lowering later splits the block's
  StatementList across a suspension point.
flags: [async]
features: [async-functions]
---*/

var outerX = 'outer';
async function bad() {
  var x = outerX;
  {
    const y = (function () { return x; })();
    await 0;
    const x = 1;
    return y;
  }
}

async function positive() {
  var a = 'outer';
  {
    let a = 1;
    await 0;
    return a;
  }
}

bad().then(
  function () { throw new Error('bad: should have rejected with a TDZ ReferenceError'); },
  function (e) { return e; }
).then(function (e) {
  assert.sameValue(
    e instanceof ReferenceError,
    true,
    'reading the shadowed name before its own declaration runs rejects with a TDZ ReferenceError'
  );
  return positive();
}).then(function (result) {
  assert.sameValue(
    result,
    1,
    'a let declared before the suspension point keeps its own, already-initialized value after resuming'
  );
}).then($DONE, $DONE);
