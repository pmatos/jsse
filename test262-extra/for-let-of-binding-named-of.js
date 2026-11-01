// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-for-in-and-for-of-statements
description: a ForDeclaration may bind the name "of"
info: |
  ForInOfStatement :
    for ( [lookahead != let] LeftHandSideExpression of AssignmentExpression ) Statement
    for ( ForDeclaration of AssignmentExpression ) Statement
  ForDeclaration : LetOrConst ForBinding
  ForBinding : BindingIdentifier

  `of` is a BindingIdentifier, so `for (let of of c)` iterates c with a
  binding named of. Only the LeftHandSideExpression form is barred from
  starting with `let`, so `for (let of c)` stays a SyntaxError.
---*/

var seen = [];
for (let of of [1, 2]) seen.push(of);
assert.sameValue(seen.join(), "1,2", "for (let of of c)");

seen = [];
for (let of in { k: 1 }) seen.push(of);
assert.sameValue(seen.join(), "k", "for (let of in o)");

seen = [];
for (let of = 3; of < 5; of++) seen.push(of);
assert.sameValue(seen.join(), "3,4", "for (let of = 3; ...)");

seen = [];
for (const of of [7]) seen.push(of);
assert.sameValue(seen.join(), "7", "for (const of of c)");

seen = [];
for (let of of [1]) { let f = () => of; seen.push(f()); }
assert.sameValue(seen.join(), "1", "binding is captured per iteration");

assert.throws(SyntaxError, function() {
  eval("for (let of [1]) ;");
}, "for (let of c)");

assert.throws(SyntaxError, function() {
  eval("for (let.x of [1]) ;");
}, "for (let.x of c)");

assert.throws(SyntaxError, function() {
  eval("'use strict'; for (let of [1]) ;");
}, "strict for (let of c)");
