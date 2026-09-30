/*---
description: >
  A nested block inside a (synchronous) generator that declares a
  shadowing let/const and also contains a suspension point (yield)
  anywhere in the block must have every lexical name of that block placed
  in TDZ, in the block's own Environment Record, from block entry --
  regardless of how many generator states the block's StatementList ends
  up split across. A read of the shadowed name that runs before the
  block's own declaration statement executes must throw a TDZ
  ReferenceError, not fall through the environment chain to an outer
  var/parameter of the same name.
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
  runs. This happens once, for the whole block, independent of whether a
  generator's state-machine lowering later splits the block's
  StatementList across a suspension point.
features: [generators]
---*/

var outerX = 'outer';
function* g() {
  var x = outerX;
  {
    const y = (function () { return x; })();
    yield; // merely being present in the block is enough
    const x = 1;
    return y;
  }
}

var it = g();
assert.throws(
  ReferenceError,
  function () { it.next(); },
  'reading the shadowed name before its own declaration runs throws a TDZ ReferenceError, ' +
    'even though the read happens in the block\'s first generator state, before the yield'
);

function* positive() {
  var a = 'outer';
  {
    let a = 1;
    yield;
    return a;
  }
}
var it2 = positive();
it2.next();
assert.sameValue(
  it2.next().value,
  1,
  'a let declared before the suspension point keeps its own, already-initialized value after resuming'
);
