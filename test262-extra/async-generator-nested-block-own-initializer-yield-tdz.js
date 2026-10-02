/*---
description: >
  A block-scoped let/const declarator inside an async generator whose own
  initializer is the suspension point itself (`const x = yield;`) must
  still get a real binding in the enclosing block's own Environment
  Record, created uninitialized (TDZ) at block entry -- not a
  function-level temporary variable assigned only once the generator
  resumes. A closure created earlier in the same block that reads the name
  before the block's own declaration statement runs must reject the
  `next()` promise with a TDZ ReferenceError, and the block's own binding
  must be a distinct storage location from an outer `var` of the same name
  in the same function.
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
  suspension point (a yield) -- so its name must still be pre-declared,
  uninitialized, before any statement of the block runs.
flags: [async]
features: [async-generators]
---*/

var outerX = 'outer';
async function* tdz() {
  var x = outerX;
  {
    const y = (function () { return x; })();
    const x = yield;
    return y;
  }
}

async function* collide() {
  var x = 1;
  {
    const x = yield;
  }
  return x;
}

var itTdz = tdz();
itTdz.next().then(
  function () { throw new Error('tdz: should have rejected with a TDZ ReferenceError'); },
  function (e) { return e; }
).then(function (e) {
  assert.sameValue(
    e instanceof ReferenceError,
    true,
    'reading the shadowed name before the block\'s own `const x = yield` ' +
      'declaration runs rejects with a TDZ ReferenceError'
  );
  var itCollide = collide();
  return itCollide.next().then(function () { return itCollide.next(999); });
}).then(function (result) {
  assert.sameValue(result.value, 1, 'the outer `var x` is untouched by the resumed value');
  assert.sameValue(result.done, true, 'the generator completes normally');
}).then($DONE, $DONE);
