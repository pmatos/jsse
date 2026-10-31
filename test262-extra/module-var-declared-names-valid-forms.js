// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-module-semantics-static-semantics-early-errors
description: var names nested in functions and block-level functions do not clash with module-level lexical names
info: |
  It is a Syntax Error if any element of the LexicallyDeclaredNames of
  ModuleItemList also occurs in the VarDeclaredNames of ModuleItemList.
  VarDeclaredNames does not look inside function bodies, and a function
  declaration in a block is lexically scoped to that block.
flags: [module]
---*/

let x = 1;
const y = 2;
class z {}

function readsOuter() {
  var x = 10;
  return x;
}

{
  let x = 3;
  var w = x;
}

{
  function y() {}
}

for (var i = 0; i < 1; i++) {}

assert.sameValue(readsOuter(), 10);
assert.sameValue(x, 1);
assert.sameValue(w, 3);
assert.sameValue(typeof z, "function");
