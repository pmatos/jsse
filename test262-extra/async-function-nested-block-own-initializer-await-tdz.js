/*---
description: >
  A block-scoped let/const declarator inside a plain async function whose
  own initializer is the suspension point itself (`const x = await p;`)
  must still get a real binding in the enclosing block's own Environment
  Record, created uninitialized (TDZ) at block entry -- not a
  function-level temporary variable assigned only once the awaited promise
  settles. A closure created earlier in the same block that reads the name
  before the block's own declaration statement runs must reject the async
  function's promise with a TDZ ReferenceError, and the block's own
  binding must be a distinct storage location from an outer `var` of the
  same name in the same function.
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

  LexicallyScopedDeclarations of a Block includes every LexicalDeclaration
  directly nested in it, including one whose own Initializer is a
  suspension point (an await) -- so its name must still be pre-declared,
  uninitialized, before any statement of the block runs.
flags: [async]
features: [async-functions]
---*/

var outerX = 'outer';
async function tdz() {
  var x = outerX;
  {
    const y = (function () { return x; })();
    const x = await 0;
    return y;
  }
}

async function collide() {
  var x = 1;
  {
    const x = await 0;
  }
  return x;
}

tdz().then(
  function () { throw new Error('tdz: should have rejected with a TDZ ReferenceError'); },
  function (e) { return e; }
).then(function (e) {
  assert.sameValue(
    e instanceof ReferenceError,
    true,
    'reading the shadowed name before the block\'s own `const x = await 0` ' +
      'declaration runs rejects with a TDZ ReferenceError'
  );
  return collide();
}).then(function (result) {
  assert.sameValue(result, 1, 'the outer `var x` is untouched by the awaited value');
}).then($DONE, $DONE);
