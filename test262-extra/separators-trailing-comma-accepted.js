// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-argument-lists
description: a single trailing comma is still permitted in argument lists, parameter lists and binding patterns
info: |
  Arguments[Yield, Await] :
    ( ArgumentList[?Yield, ?Await] , )

  FormalParameters[Yield, Await] :
    FormalParameterList[?Yield, ?Await] ,

  ArrayBindingPattern and ObjectBindingPattern admit a trailing comma after
  the last element, and template substitutions are unaffected.
---*/

function sum(a, b,) { return a + b; }
assert.sameValue(sum(1, 2,), 3, "call and parameter trailing commas");
assert.sameValue(new Array(1, 2,).length, 2, "new arguments trailing comma");

var [x, , y,] = [1, 2, 3];
assert.sameValue(x, 1, "array pattern first");
assert.sameValue(y, 3, "array pattern after elision");

var { p: q, } = { p: 5 };
assert.sameValue(q, 5, "object pattern trailing comma");

assert.sameValue(`a${1}b${2}c`, "a1b2c", "template substitutions");
assert.sameValue(`${`${1}`}`, "1", "nested template");
