// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-proxy-object-internal-methods-and-internal-slots-get-p-receiver
description: >
  A Symbol property key handed to a Proxy trap is the very Symbol the program
  used: identity and [[Description]] (undefined, the empty string, a lone
  surrogate, text that looks like a well-known name or an id suffix) survive
  every trap, repeated conversions of one Symbol agree, and a Symbol that is
  only reachable as a key on an object keeps its description after the
  original value is dropped.
features: [Symbol, Proxy, Reflect, Symbol.prototype.description]
---*/

var named = Symbol('mobx administration');
var empty = Symbol('');
var none = Symbol();
var lone = Symbol('a\ud800b');
var lookalike = Symbol('Symbol.iterator');
var suffix = Symbol('x)#9');
var registered = Symbol.for('registered key');
var all = [named, empty, none, lone, lookalike, suffix, registered, Symbol.iterator, Symbol.toPrimitive];

function check(key, expected, label) {
  assert.sameValue(typeof key, 'symbol', label + ': is a symbol');
  assert.sameValue(key, expected, label + ': identity');
  assert.sameValue(key.description, expected.description, label + ': description');
  assert.sameValue(key.toString(), expected.toString(), label + ': toString');
}

var seen;
var handler = {
  get: function(t, k, r) { seen = k; return Reflect.get(t, k, r); },
  set: function(t, k, v, r) { seen = k; return Reflect.set(t, k, v, r); },
  has: function(t, k) { seen = k; return Reflect.has(t, k); },
  deleteProperty: function(t, k) { seen = k; return Reflect.deleteProperty(t, k); },
  defineProperty: function(t, k, d) { seen = k; return Reflect.defineProperty(t, k, d); },
  getOwnPropertyDescriptor: function(t, k) { seen = k; return Reflect.getOwnPropertyDescriptor(t, k); }
};
var target = {};
var proxy = new Proxy(target, handler);

all.forEach(function(sym, i) {
  var label = 'symbol #' + i;
  seen = undefined;
  proxy[sym] = i;
  check(seen, sym, label + ' set');
  seen = undefined;
  assert.sameValue(proxy[sym], i, label + ' get value');
  check(seen, sym, label + ' get');
  seen = undefined;
  assert.sameValue(sym in proxy, true, label + ' has value');
  check(seen, sym, label + ' has');
  seen = undefined;
  Object.getOwnPropertyDescriptor(proxy, sym);
  check(seen, sym, label + ' getOwnPropertyDescriptor');
  seen = undefined;
  Object.defineProperty(proxy, sym, { value: i + 100, configurable: true });
  check(seen, sym, label + ' defineProperty');
  seen = undefined;
  assert.sameValue(delete proxy[sym], true, label + ' delete value');
  check(seen, sym, label + ' deleteProperty');
});

var ownKeysProxy = new Proxy({}, {
  ownKeys: function() { return all.slice(); }
});
var keys = Reflect.ownKeys(ownKeysProxy);
assert.sameValue(keys.length, all.length, 'ownKeys trap result length');
all.forEach(function(sym, i) { check(keys[i], sym, 'ownKeys result #' + i); });

var plain = {};
all.forEach(function(sym) { plain[sym] = 1; });
var plainKeys = Reflect.ownKeys(plain);
all.forEach(function(sym, i) { check(plainKeys[i], sym, 'ownKeys of a plain object #' + i); });

var userA = Symbol('Symbol.iterator');
var userB = Symbol('Symbol.iterator');
var holder = {};
holder[userA] = 'a';
holder[userB] = 'b';
holder[Symbol.iterator] = 'wk';
assert.sameValue(holder[userA], 'a', 'user symbol with well-known description, first');
assert.sameValue(holder[userB], 'b', 'user symbol with well-known description, second');
assert.sameValue(holder[Symbol.iterator], 'wk', 'the real well-known symbol');
assert.sameValue(holder[userA], 'a', 'a repeated read converts to the same key');

var transient = {};
(function() {
  transient[Symbol('only reachable as a key')] = 1;
  transient[Symbol('')] = 2;
  transient[Symbol()] = 3;
  transient[Symbol('lone\ud800')] = 4;
})();
if (typeof $262 !== 'undefined' && $262.gc) {
  $262.gc();
}
var transientKeys = Object.getOwnPropertySymbols(transient);
assert.sameValue(transientKeys.length, 4, 'transient symbol keys survive their symbols');
assert.sameValue(transientKeys[0].description, 'only reachable as a key', 'transient description');
assert.sameValue(transientKeys[1].description, '', 'transient empty description');
assert.sameValue(transientKeys[2].description, undefined, 'transient undefined description');
assert.sameValue(transientKeys[3].description, 'lone\ud800', 'transient lone surrogate description');
assert.sameValue(transient[transientKeys[0]], 1, 'transient key reads back its value');
assert.sameValue(transient[transientKeys[3]], 4, 'transient lone-surrogate key reads back its value');
assert.sameValue(transientKeys[0], Object.getOwnPropertySymbols(transient)[0], 'rebuilt key symbols are stable');
