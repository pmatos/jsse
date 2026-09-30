/*---
description: >
  A try statement's try-block is a Block per grammar
  (TryStatement : try Block Finally), so it gets its own
  BlockDeclarationInstantiation the same as any other block. A shadowing
  let/const declared in the try-block, with a suspension point (yield)
  anywhere in it, must keep the shadowed name in TDZ from block entry,
  even when the generator's state-machine lowering splits the try-block's
  StatementList across the yield.
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

  A try-block is evaluated the same way as any other Block
  (sec-try-statement-runtime-semantics-evaluation), so its own lexical
  declarations must all enter TDZ together, before any of its statements
  run -- independent of how the generator's state-machine lowering splits
  it across a suspension point.
features: [generators]
---*/

var outerX = 'outer';
function* g() {
  var x = outerX;
  try {
    const y = (function () { return x; })();
    yield;
    const x = 1;
    return y;
  } finally {
  }
}

var it = g();
assert.throws(
  ReferenceError,
  function () { it.next(); },
  'reading the shadowed name before its own declaration runs in the try-block throws a TDZ ReferenceError'
);

function* positive() {
  var a = 'outer';
  try {
    let a = 1;
    yield;
    return a;
  } finally {
  }
}
var it2 = positive();
it2.next();
assert.sameValue(
  it2.next().value,
  1,
  'a let declared before the suspension point in the try-block keeps its own value after resuming'
);
