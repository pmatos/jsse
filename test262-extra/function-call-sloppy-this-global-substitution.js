// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-ordinarycallbindthis
description: >
  OrdinaryCallBindThis: a sloppy function called with an undefined or null
  `this` sees the callee realm's global this value; primitives are wrapped by
  ToObject; strict functions see `this` untouched. A function from another
  realm uses its own realm's global.
info: |
  10.2.1.2 OrdinaryCallBindThis ( F, calleeContext, thisArgument )
    5. If thisMode is strict, let thisValue be thisArgument.
    6. Else,
      a. If thisArgument is either undefined or null, then
        i. Let calleeRealm be F.[[Realm]].
        ii. Let globalEnv be calleeRealm.[[GlobalEnv]].
        iii. Let thisValue be globalEnv.[[GlobalThisValue]].
      b. Else, let thisValue be ! ToObject(thisArgument).
flags: [noStrict]
features: [cross-realm]
---*/

var theGlobal = this;

function sloppyThis() { return this; }
function strictThis() { "use strict"; return this; }

assert.sameValue(sloppyThis(), theGlobal, "implicit undefined this");
assert.sameValue(sloppyThis.call(undefined), theGlobal, "explicit undefined");
assert.sameValue(sloppyThis.call(null), theGlobal, "explicit null");
assert.sameValue(sloppyThis.apply(undefined, []), theGlobal, "apply undefined");
assert.sameValue((0, sloppyThis)(), theGlobal, "indirect call");
assert.sameValue(sloppyThis.bind(undefined)(), theGlobal, "bound undefined");

var wrappedNumber = sloppyThis.call(5);
assert.sameValue(typeof wrappedNumber, "object", "number is wrapped");
assert.sameValue(wrappedNumber instanceof Number, true, "number wrapper type");
assert.sameValue(typeof sloppyThis.call("s"), "object", "string is wrapped");
assert.sameValue(typeof sloppyThis.call(true), "object", "boolean is wrapped");
assert.sameValue(typeof sloppyThis.call(Symbol()), "object", "symbol is wrapped");

assert.sameValue(strictThis(), undefined, "strict undefined");
assert.sameValue(strictThis.call(null), null, "strict null");
assert.sameValue(strictThis.call(5), 5, "strict primitive untouched");

var o = {};
assert.sameValue(sloppyThis.call(o), o, "object this untouched");

var other = $262.createRealm().global;
var otherSloppy = other.eval("(function() { return this; })");
assert.sameValue(otherSloppy(), other, "other realm function sees its own global");
assert.notSameValue(otherSloppy(), theGlobal, "not the caller's global");
assert.sameValue(otherSloppy.call(undefined), other, "call(undefined) cross realm");
assert.sameValue(otherSloppy.call(null), other, "call(null) cross realm");

var otherArrow = other.eval("(function() { return () => this; })");
assert.sameValue(otherArrow()(), other, "arrow inherits other realm this");

var otherStrict = other.eval("(function() { 'use strict'; return this; })");
assert.sameValue(otherStrict(), undefined, "cross realm strict untouched");
