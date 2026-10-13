/*---
description: Computed element reads and writes on dense Arrays honour holes, attributes, integrity levels and inherited properties
info: |
  Element access runs from function bodies so it reaches the bytecode VM's
  numeric-index GetElement/SetElement fast paths, which must be
  observationally identical to the generic [[Get]] and [[Set]]: holes are
  distinct from present undefined, attributes and integrity levels are
  honoured, and inherited setters or read-only properties on any prototype
  still run.

  10.1.9.2 OrdinarySetWithOwnDescriptor
  10.4.2.1 Array [[DefineOwnProperty]]
  7.3.16 SetIntegrityLevel
esid: sec-ordinarysetwithowndescriptor
includes: []
flags: [noStrict]
features: [Reflect, Proxy, class, cross-realm]
---*/

function store(a, i, v) { a[i] = v; return a; }
function storeStrict(a, i, v) { "use strict"; a[i] = v; return a; }
function load(a, i) { return a[i]; }

// Overwrite, append, hole-fill and length bookkeeping.
(function () {
  var a = [1, 2, 3];
  for (var i = 0; i < 3; i++) store(a, i, i * 10);
  assert.sameValue(a.join(), '0,10,20', 'in-bounds overwrite');
  assert.sameValue(a.length, 3, 'overwrite leaves length alone');

  store(a, 3, 30);
  assert.sameValue(a.length, 4, 'append grows length');
  assert.sameValue(load(a, 3), 30, 'appended value readable');

  var h = new Array(4);
  store(h, 2, 'x');
  assert.sameValue(h.length, 4, 'hole-fill inside length leaves length alone');
  assert.sameValue(2 in h, true, 'filled hole is present');
  assert.sameValue(1 in h, false, 'untouched hole stays a hole');
  assert.sameValue(load(h, 1), undefined, 'hole reads undefined');

  var s = [];
  store(s, 0, 'a');
  store(s, 1, 'b');
  assert.sameValue(s.length, 2, 'append to empty array');
  store(s, 5, 'f');
  assert.sameValue(s.length, 6, 'sparse write grows length');
  assert.sameValue(3 in s, false, 'gap is not materialised');

  var z = [0];
  store(z, -0, 'zero');
  assert.sameValue(z[0], 'zero', '-0 addresses element zero');
  assert.sameValue(z.length, 1, '-0 does not append');
})();

// Present undefined is not a hole.
(function () {
  var a = [1, 2, 3];
  store(a, 1, undefined);
  assert.sameValue(1 in a, true, 'stored undefined is present');
  assert.sameValue(a.hasOwnProperty(1), true, 'stored undefined is an own property');
  assert.sameValue(Object.keys(a).join(), '0,1,2', 'stored undefined is enumerated');
  store(a, 1, 7);
  assert.sameValue(a[1], 7, 'overwrite of present undefined');
  assert.sameValue(Object.getOwnPropertyDescriptor(a, 1).writable, true, 'attributes kept');

  var b = [1, , 3];
  assert.sameValue(load(b, 1), undefined, 'hole read');
  assert.sameValue(1 in b, false, 'hole is absent');
  store(b, 1, undefined);
  assert.sameValue(1 in b, true, 'writing undefined into a hole makes it present');

  var c = [];
  store(c, 0, undefined);
  assert.sameValue(c.length, 1, 'appended undefined grows length');
  assert.sameValue(0 in c, true, 'appended undefined is present');
})();

// Reads honour accessors and non-default attributes defined on an element.
(function () {
  var a = [1, 2, 3];
  var calls = 0;
  Object.defineProperty(a, 1, { get: function () { calls++; return 'got'; }, configurable: true });
  assert.sameValue(load(a, 1), 'got', 'getter on element');
  assert.sameValue(calls, 1, 'getter ran once');
  store(a, 1, 'ignored');
  assert.sameValue(load(a, 1), 'got', 'write to getter-only element is ignored');
  assert.throws(TypeError, function () { storeStrict(a, 1, 'x'); }, 'strict write to getter-only element');

  var b = [1, 2, 3];
  var seen;
  Object.defineProperty(b, 2, { set: function (v) { seen = v; }, configurable: true });
  store(b, 2, 'via setter');
  assert.sameValue(seen, 'via setter', 'setter on element runs');
  assert.sameValue(load(b, 2), undefined, 'setter-only element reads undefined');

  var c = [1, 2, 3];
  Object.defineProperty(c, 0, { value: 'ro', writable: false, enumerable: true, configurable: true });
  store(c, 0, 'rw');
  assert.sameValue(load(c, 0), 'ro', 'non-writable element is not overwritten');
  assert.throws(TypeError, function () { storeStrict(c, 0, 'x'); }, 'strict write to non-writable element');
  store(c, 1, 'ok');
  assert.sameValue(load(c, 1), 'ok', 'sibling elements stay writable');
})();

// Integrity levels.
(function () {
  var f = Object.freeze([1, 2, 3]);
  store(f, 0, 'x');
  store(f, 3, 'y');
  assert.sameValue(f.join(), '1,2,3', 'sloppy writes to a frozen array are ignored');
  assert.sameValue(f.length, 3, 'frozen length unchanged');
  assert.throws(TypeError, function () { storeStrict(f, 0, 'x'); }, 'strict overwrite of frozen element');
  assert.throws(TypeError, function () { storeStrict(f, 3, 'x'); }, 'strict append to frozen array');

  var s = Object.seal([1, 2, 3]);
  store(s, 1, 'ok');
  assert.sameValue(s[1], 'ok', 'sealed elements stay writable');
  store(s, 3, 'no');
  assert.sameValue(s.length, 3, 'sealed array cannot grow');
  assert.throws(TypeError, function () { storeStrict(s, 3, 'x'); }, 'strict append to sealed array');

  var p = Object.preventExtensions([1, , 3]);
  store(p, 0, 'ok');
  assert.sameValue(p[0], 'ok', 'non-extensible overwrite of a present element');
  store(p, 1, 'no');
  assert.sameValue(1 in p, false, 'non-extensible array cannot fill a hole');
  store(p, 3, 'no');
  assert.sameValue(p.length, 3, 'non-extensible array cannot grow');
  assert.throws(TypeError, function () { storeStrict(p, 1, 'x'); }, 'strict hole-fill on non-extensible array');

  var l = [1, 2];
  Object.defineProperty(l, 'length', { writable: false });
  store(l, 0, 'ok');
  assert.sameValue(l[0], 'ok', 'overwrite with read-only length');
  store(l, 2, 'no');
  assert.sameValue(l.length, 2, 'read-only length blocks append');
  assert.sameValue(2 in l, false, 'no element created past read-only length');
  assert.throws(TypeError, function () { storeStrict(l, 2, 'x'); }, 'strict append past read-only length');
})();

// Inherited setters and read-only properties apply to holes and appends. Each
// prototype is polluted in a fresh realm so that its cleanup is the only thing
// that can restore the fast paths there.
(function () {
  ['Array', 'Object'].forEach(function (name) {
    var g = $262.createRealm().global;
    var proto = g[name].prototype;
    var log = '';
    var count = 0;
    Object.defineProperty(proto, 1, {
      set: function (v) { log += v + ';'; count++; },
      get: function () { return 'inherited'; },
      configurable: true
    });
    var a = g.Array.of(0);
    store(a, 1, 'appended');
    assert.sameValue(log, 'appended;', name + '.prototype setter intercepts append');
    assert.sameValue(a.length, 1, name + '.prototype setter: append creates nothing');
    assert.sameValue(load(a, 1), 'inherited', name + '.prototype getter read');

    var h = new g.Array(3);
    store(h, 1, 'hole');
    assert.sameValue(log, 'appended;hole;', name + '.prototype setter intercepts hole fill');
    assert.sameValue(h.hasOwnProperty(1), false, name + '.prototype setter: hole not materialised');

    var o = g.Array.of(0, 5);
    store(o, 1, 'own');
    assert.sameValue(o[1], 'own', 'own element wins over inherited accessor');
    assert.sameValue(count, 2, 'own overwrite does not call the setter');
    delete proto[1];

    Object.defineProperty(proto, 2, { value: 'ro', writable: false, configurable: true });
    var r = g.Array.of(0, 1);
    store(r, 2, 'x');
    assert.sameValue(r.length, 2, name + '.prototype read-only index blocks append');
    assert.sameValue(load(r, 2), 'ro', name + '.prototype data property read');
    assert.throws(TypeError, function () { storeStrict(r, 2, 'x'); }, 'strict append over inherited read-only');
    delete proto[2];

    Object.defineProperty(proto, 0, { value: 'shadow', writable: true, configurable: true });
    var d = new g.Array();
    store(d, 0, 'mine');
    assert.sameValue(Object.getOwnPropertyDescriptor(d, 0).value, 'mine', 'writable inherited data property: own element created');
    assert.sameValue(d.length, 1, 'length after create');
    delete proto[0];

    var after = new g.Array();
    store(after, 0, 'again');
    assert.sameValue(after.length, 1, 'append works after prototype cleanup');
  });
})();

// A Proxy anywhere on the prototype chain sees the write.
(function () {
  var trapped = [];
  var proto = new Proxy({}, {
    set: function (target, key, value, receiver) {
      trapped.push(String(key) + '=' + String(value));
      return Reflect.set(target, key, value, receiver);
    }
  });
  var a = [0];
  Object.setPrototypeOf(a, proto);
  store(a, 1, 'p');
  assert.sameValue(trapped.join(), '1=p', 'proxy prototype set trap runs for append');
  assert.sameValue(a[1], 'p', 'Reflect.set with receiver created the element');
  store(a, 0, 'own');
  assert.sameValue(trapped.length, 1, 'overwrite of an own element does not consult the proxy');

  var b = [0];
  Object.setPrototypeOf(b, Object.create(proto));
  store(b, 1, 'deep');
  assert.sameValue(trapped.length, 2, 'proxy two links up the chain');

  var holey = new Array(3);
  Object.setPrototypeOf(holey, proto);
  store(holey, 1, 'h');
  assert.sameValue(trapped[2], '1=h', 'proxy prototype set trap runs for hole fill');
})();

// Receivers that are not plain Arrays on the chain.
(function () {
  var str = new String('abc');
  var a = [];
  Object.setPrototypeOf(a, str);
  store(a, 0, 'z');
  assert.sameValue(a.length, 0, 'String wrapper prototype: read-only character blocks the write');
  assert.sameValue(load(a, 0), 'a', 'String wrapper prototype: character visible');
  assert.throws(TypeError, function () { storeStrict(a, 1, 'x'); }, 'String wrapper prototype: strict write rejected');

  var boxed = [];
  Object.setPrototypeOf(boxed, Object('abc'));
  store(boxed, 0, 'z');
  assert.sameValue(boxed.length, 0, 'boxed string prototype: read-only character blocks the write');
  assert.throws(TypeError, function () { storeStrict(boxed, 0, 'x'); }, 'boxed string prototype: strict write rejected');

  var ta = new Uint8Array(2);
  var b = [];
  Object.setPrototypeOf(b, ta);
  store(b, 5, 'x');
  assert.sameValue(b.length, 0, 'TypedArray prototype swallows an out-of-range write');

  class Sub extends Array {}
  var s = new Sub();
  store(s, 0, 'a');
  store(s, 1, 'b');
  assert.sameValue(s.length, 2, 'Array subclass appends');
  assert.sameValue(s instanceof Sub, true, 'subclass identity kept');
  store(s, 0, 'c');
  assert.sameValue(s.join(), 'c,b', 'Array subclass overwrite');
})();

// Named properties alongside elements, and index-like names.
(function () {
  var a = [1, 2, 3];
  a.foo = 'bar';
  a['01'] = 'octal-like';
  a['1.5'] = 'fraction';
  assert.sameValue(load(a, 1), 2, 'named properties do not disturb element reads');
  store(a, 1, 'two');
  assert.sameValue(a[1], 'two', 'overwrite with named properties present');
  store(a, 3, 'four');
  assert.sameValue(a.length, 4, 'append with named properties present');
  assert.sameValue(a['01'], 'octal-like', 'non-canonical index name untouched');
  assert.sameValue(load(a, 1.5), 'fraction', 'fractional key reads the named property');

  var b = [];
  b[4294967295] = 'not an index';
  assert.sameValue(b.length, 0, '2^32-1 is not an array index');
  b[4294967294] = 'last index';
  assert.sameValue(b.length, 4294967295, 'largest index sets length to 2^32-1');
  store(b, 0, 'first');
  assert.sameValue(b[0], 'first', 'element zero of a huge sparse array');
})();

// Non-canonical numeric strings are ordinary property names, not elements.
(function () {
  var a = [1, 2, 3];
  assert.sameValue(a['01'], undefined, 'leading zero read');
  assert.sameValue(a['+1'], undefined, 'plus sign read');
  assert.sameValue(a['1.0'], undefined, 'trailing zero read');
  assert.sameValue('01' in a, false, 'leading zero is not present');
  assert.sameValue(a.hasOwnProperty('+1'), false, 'plus sign is not an own property');
  assert.sameValue(delete a['01'], true, 'deleting an absent non-canonical name succeeds');
  assert.sameValue(a[1], 2, 'delete of "01" leaves element one alone');
  assert.sameValue(a.length, 3, 'delete of "01" leaves length alone');
  a['01'] = 'named';
  assert.sameValue(a[1], 2, 'write to "01" leaves element one alone');
  assert.sameValue(a['01'], 'named', 'write to "01" is an ordinary property');
  assert.sameValue(Object.keys(a).join(), '0,1,2,01', 'keys list the element indices first');
  a['04'] = 'named4';
  assert.sameValue(a.length, 3, 'write to "04" does not grow the array');
  assert.sameValue(4 in a, false, 'write to "04" does not create element four');
  assert.sameValue(Reflect.deleteProperty(a, '01'), true, 'Reflect.deleteProperty of "01"');
  assert.sameValue(a[1], 2, 'Reflect.deleteProperty of "01" leaves element one alone');
  assert.sameValue(store(Array.of(7, 8, 9), '02', 'x')[2], 9, 'created array: write to "02"');
})();

// Non-number keys and out-of-range numbers use the generic path unchanged.
(function () {
  var a = [1, 2, 3];
  assert.sameValue(load(a, '1'), 2, 'string key read');
  assert.sameValue(load(a, 1.0), 2, 'integral double read');
  assert.sameValue(load(a, 3), undefined, 'one past the end');
  assert.sameValue(load(a, -1), undefined, 'negative index read');
  assert.sameValue(load(a, NaN), undefined, 'NaN read');
  store(a, -1, 'neg');
  assert.sameValue(a[-1], 'neg', 'negative index creates a named property');
  assert.sameValue(a.length, 3, 'negative index does not touch length');
  store(a, 1.5, 'frac');
  assert.sameValue(a['1.5'], 'frac', 'fractional index creates a named property');
  store(a, '2', 'str');
  assert.sameValue(a[2], 'str', 'string key overwrite');
})();

// A stored object keeps its identity and survives a collection.
(function () {
  var a = new Array(64);
  for (var i = 0; i < 64; i++) store(a, i, { n: i });
  if (typeof $262 !== 'undefined' && $262.gc) $262.gc();
  for (var j = 0; j < 64; j++) assert.sameValue(load(a, j).n, j, 'object element ' + j);
})();
