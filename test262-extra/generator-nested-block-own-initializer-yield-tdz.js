/*---
description: >
  A block-scoped let/const declarator whose own initializer is the
  suspension point itself (`const x = yield;`) must still get a real
  binding in the enclosing block's own Environment Record, created
  uninitialized (TDZ) at block entry like every other lexically scoped
  declaration of the block -- not a function-level temporary variable
  assigned only once the generator resumes. Two consequences follow from
  getting this right: (1) a closure created earlier in the same block that
  reads the name before the block's own declaration statement runs must
  observe TDZ, and (2) the block's own binding must be a distinct storage
  location from an outer `var` of the same name in the same function, so
  resuming the generator does not clobber the outer `var`.
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
  suspension point -- the declarator's own initializer being a yield does
  not remove it from the block's static lexical scan, so its name must
  still be pre-declared, uninitialized, before any statement of the block
  runs.
features: [generators]
---*/

// 1. No TDZ bypass: a closure created before the block's own
// `const x = yield` must observe TDZ on `x`, not read through to an outer
// `var` of the same name (there is none in scope chain terms -- the read
// must throw because the block's own `x` binding exists, uninitialized,
// from block entry).
var outerX = 'outer';
function* tdz() {
  var x = outerX;
  {
    const y = (function () { return x; })();
    const x = yield;
    return y;
  }
}

var itTdz = tdz();
assert.throws(
  ReferenceError,
  function () { itTdz.next(); },
  'reading the shadowed name before the block\'s own `const x = yield` ' +
    'declaration runs throws a TDZ ReferenceError'
);

// 2. Identity: the block-scoped `const x` and the outer `var x` must be
// different storage locations, so resuming the generator does not
// overwrite the outer `x`.
function* collide() {
  var x = 1;
  {
    const x = yield;
  }
  return x;
}

var itCollide = collide();
itCollide.next();
var result = itCollide.next(999);
assert.sameValue(result.value, 1, 'the outer `var x` is untouched by the resumed value');
assert.sameValue(result.done, true, 'the generator completes normally');

// 3. Positive case: the block's own `const` correctly observes the sent
// value once the fix routes it through a genuine binding.
function* positive() {
  {
    const x = yield;
    return x;
  }
}

var itPositive = positive();
itPositive.next();
assert.sameValue(
  itPositive.next(42).value,
  42,
  'the block-scoped const observes the value sent into the suspending initializer'
);
