// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-proxy-object-internal-methods-and-internal-slots-get-p-receiver
description: >
  A String property key handed to a Proxy trap is the very String value the
  program used (=== the original, any code units including lone surrogates and
  non-BMP pairs), and the trap's own Reflect.get / Reflect.has / Reflect.set /
  Reflect.deleteProperty on that value address the same property. Repeated
  conversions, keys that only differ by a suffix, index keys on both sides of
  the small-index table and over-long keys all agree.
features: [Proxy, Reflect]
---*/

var seenGet, seenHas, seenSet, seenDelete;
var handler = {
  get: function(t, k, r) { seenGet = k; return Reflect.get(t, k, r); },
  has: function(t, k) { seenHas = k; return Reflect.has(t, k); },
  set: function(t, k, v, r) { seenSet = k; return Reflect.set(t, k, v, r); },
  deleteProperty: function(t, k) { seenDelete = k; return Reflect.deleteProperty(t, k); }
};

var long = '';
for (var i = 0; i < 300; i++) long += 'k';

var keys = [
  'x', 'name', 'length', '', ' ', 'a\ud800b', '\udc00', '\ud800', 'a😀b',
  '😀', '\ud83d', '\ude00\ud83d', 'é', '€', '\0', 'Symbol(x)#1',
  '01', '1.5', '-1', '4294967295', '4294967296', 'constructorx', '__proto__x',
  long, long + 'z'
];
for (var n = 0; n < 130; n++) keys.push(String(n));
keys.push('4294967294');

function same(a, b, label) {
  assert.sameValue(typeof a, 'string', label + ': is a string');
  assert.sameValue(a === b, true, label + ': ===');
  assert.sameValue(a.length, b.length, label + ': length');
  for (var j = 0; j < b.length; j++) {
    assert.sameValue(a.charCodeAt(j), b.charCodeAt(j), label + ': unit ' + j);
  }
}

for (var round = 0; round < 3; round++) {
  var target = {};
  var proxy = new Proxy(target, handler);
  keys.forEach(function(key, idx) {
    var label = 'round ' + round + ' key #' + idx;
    seenSet = undefined;
    proxy[key] = idx;
    same(seenSet, key, label + ' set');
    seenGet = undefined;
    assert.sameValue(proxy[key], idx, label + ' get value');
    same(seenGet, key, label + ' get');
    seenHas = undefined;
    assert.sameValue(key in proxy, true, label + ' has value');
    same(seenHas, key, label + ' has');
    assert.sameValue(Object.prototype.hasOwnProperty.call(target, key), true, label + ' lands on the target');
    assert.sameValue(Object.keys(target).indexOf(key) >= 0, true, label + ' listed by Object.keys');
  });

  keys.forEach(function(key, idx) {
    var label = 'round ' + round + ' key #' + idx;
    seenDelete = undefined;
    assert.sameValue(delete proxy[key], true, label + ' delete value');
    same(seenDelete, key, label + ' delete');
    assert.sameValue(Object.prototype.hasOwnProperty.call(target, key), false, label + ' removed from the target');
    seenHas = undefined;
    assert.sameValue(key in proxy, false, label + ' has after delete');
    same(seenHas, key, label + ' has after delete trap argument');
  });
  assert.sameValue(Reflect.ownKeys(target).length, 0, 'round ' + round + ': target is empty');
}

var base = {};
var viaProxy = new Proxy(base, handler);
base.ab = 'ab-value';
base.abc = 'abc-value';
base['ab\ud800'] = 'lone-value';
var stem = 'a';
stem += 'b';
var longer = stem + 'c';
assert.sameValue(viaProxy[stem], 'ab-value', 'grown string stem');
assert.sameValue(viaProxy[longer], 'abc-value', 'grown string extended past a key seen by the trap');
assert.sameValue(viaProxy[stem + '\ud800'], 'lone-value', 'grown string with a lone surrogate suffix');
assert.sameValue(viaProxy[stem], 'ab-value', 'stem again');
same(seenGet, stem, 'stem trap argument');

var names = Object.keys({ first: 1, second: 2, third: 3 });
var holder = { first: 'one', second: 'two', third: 'three' };
var holderProxy = new Proxy(holder, handler);
names.forEach(function(name) {
  assert.sameValue(holderProxy[name], holder[name], name + ' through a key obtained from Object.keys');
  same(seenGet, name, name + ' trap argument');
  assert.sameValue(Reflect.get(holder, seenGet), holder[name], name + ' Reflect.get on the trap argument');
});

var order = [];
var keyObject = { toString: function() { order.push('toString'); return 'converted'; } };
var orderProxy = new Proxy({ converted: 'ok' }, {
  get: function(t, k, r) { order.push('get:' + typeof k); return Reflect.get(t, k, r); }
});
assert.sameValue(orderProxy[keyObject], 'ok', 'object key is converted once');
assert.sameValue(order.join(), 'toString,get:string', 'ToPropertyKey runs before the trap, which sees a String');
