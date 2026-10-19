// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-for-in-and-for-of-statements
description: >
  A for-in/for-of head binds a pattern through ForBinding, which takes no
  Initializer, so the initializer requirement of var/let/const declarations
  does not apply to it.
info: |
  ForDeclaration[Yield, Await] :
    LetOrConst ForBinding[?Yield, ?Await]

  ForBinding[Yield, Await] :
    BindingIdentifier[?Yield, ?Await]
    BindingPattern[?Yield, ?Await]

  for ( [lookahead != let [] var ForBinding[?Yield, ?Await] of ... ) Statement
---*/

var seen = [];

for (var {length: len} of ["ab", "cde"]) { seen.push(len); }
assert.sameValue(seen.join(), "2,3", "for (var {..} of ..)");

seen = [];
for (let [first] of ["xy", "zw"]) { seen.push(first); }
assert.sameValue(seen.join(), "x,z", "for (let [..] of ..)");

seen = [];
for (const {length} of [[1], [1, 2]]) { seen.push(length); }
assert.sameValue(seen.join(), "1,2", "for (const {..} of ..)");

var keys = [];
for (var [k] in {ab: 1, cd: 2}) { keys.push(k); }
assert.sameValue(keys.join(), "a,c", "for (var [..] in ..)");

keys = [];
for (let {0: k0} in {ab: 1}) { keys.push(k0); }
assert.sameValue(keys.join(), "a", "for (let {..} in ..)");

var init = [];
for (var [x] = ["init"]; init.length < 1; init.push(x)) {}
assert.sameValue(init[0], "init", "for (var [..] = ..;;) keeps its initializer");

var fns = [];
for (const { f = () => 1 } in { k: 1 }) { fns.push(f); }
assert.sameValue(fns.length, 1, "pattern defaults containing arrows in a for-in head");
assert.sameValue(fns[0](), 1, "the arrow default is bound");

var results = [];
for (const [a, g = () => a] of ["z"]) { results.push(g()); }
assert.sameValue(results.join(), "z", "pattern defaults containing arrows in a for-of head");
