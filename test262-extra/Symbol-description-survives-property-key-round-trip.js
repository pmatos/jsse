// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-symbol.prototype.description
description: >
  A Symbol's [[Description]] (undefined, the empty string, a lone surrogate)
  survives being used as a property key and read back, and SymbolDescriptiveString
  and SetFunctionName keep lone surrogates and the empty description.
features: [Symbol, Symbol.prototype.description, Proxy, Reflect, class-static-fields-public]
---*/

var empty = Symbol('');
var lone = Symbol('a\ud800b');
var none = Symbol();
var hash = Symbol('x)#9');

function holder(sym) {
  var o = {};
  o[sym] = 1;
  return o;
}

var back = Object.getOwnPropertySymbols(holder(empty))[0];
assert.sameValue(back, empty, "empty: identity via getOwnPropertySymbols");
assert.sameValue(back.description, "", "empty: description via getOwnPropertySymbols");
assert.sameValue(Reflect.ownKeys(holder(empty))[0].description, "", "empty: description via Reflect.ownKeys");
assert.sameValue(Object.getOwnPropertySymbols({ ...holder(empty) })[0].description, "", "empty: description via spread");
assert.sameValue(Object.getOwnPropertySymbols(Object.assign({}, holder(empty)))[0].description, "", "empty: description via Object.assign");

assert.sameValue(Object.getOwnPropertySymbols(holder(lone))[0].description, 'a\ud800b', "lone: description via getOwnPropertySymbols");
assert.sameValue(Reflect.ownKeys(holder(lone))[0].description, 'a\ud800b', "lone: description via Reflect.ownKeys");
assert.sameValue(Object.getOwnPropertySymbols(holder(none))[0].description, undefined, "undefined description stays undefined");
assert.sameValue(Object.getOwnPropertySymbols(holder(hash))[0].description, 'x)#9', "description containing )# is preserved");

var seen;
var proxy = new Proxy({}, {
  get: function(t, k) { seen = k; return 1; },
  has: function(t, k) { seen = k; return true; }
});
proxy[empty];
assert.sameValue(seen.description, "", "empty: Proxy get trap key description");
seen = undefined;
empty in proxy;
assert.sameValue(seen.description, "", "empty: Proxy has trap key description");
proxy[lone];
assert.sameValue(seen.description, 'a\ud800b', "lone: Proxy get trap key description");
proxy[none];
assert.sameValue(seen.description, undefined, "Proxy get trap key keeps undefined description");

assert.sameValue(lone.toString(), 'Symbol(a\ud800b)', "Symbol.prototype.toString keeps lone surrogate");
assert.sameValue(String(lone), 'Symbol(a\ud800b)', "String(symbol) keeps lone surrogate");
assert.sameValue(empty.toString(), 'Symbol()', "empty description toString");
assert.sameValue(none.toString(), 'Symbol()', "undefined description toString");

assert.sameValue({ [lone]: function() {} }[lone].name, '[a\ud800b]', "object literal function name");
assert.sameValue({ [empty]: function() {} }[empty].name, '[]', "empty description function name");
assert.sameValue({ [none]: function() {} }[none].name, '', "undefined description function name");
assert.sameValue({ [lone]() {} }[lone].name, '[a\ud800b]', "object literal method name");

class C {
  static [lone] = function() {};
  [lone]() {}
  static [empty] = function() {};
}
assert.sameValue(C[lone].name, '[a\ud800b]', "static field function name");
assert.sameValue(C.prototype[lone].name, '[a\ud800b]', "class method name");
assert.sameValue(C[empty].name, '[]', "static field function name with empty description");

class D {
  [lone] = function() {};
  [empty] = () => {};
}
var d = new D();
assert.sameValue(d[lone].name, '[a\ud800b]', "instance field function name");
assert.sameValue(d[empty].name, '[]', "instance field function name with empty description");
