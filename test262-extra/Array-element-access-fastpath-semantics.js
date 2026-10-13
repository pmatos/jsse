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
features: [Reflect, Proxy, class]
---*/

function store(a, i, v) { a[i] = v; return a; }
function storeStrict(a, i, v) { "use strict"; a[i] = v; return a; }
function load(a, i) { return a[i]; }

function assertSame(actual, expected, message) {
  if (actual !== expected && !(actual !== actual && expected !== expected)) {
    throw new Test262Error(message + ': expected ' + String(expected) + ', got ' + String(actual));
  }
}

function assertThrows(Ctor, fn, message) {
  try {
    fn();
  } catch (e) {
    if (!(e instanceof Ctor)) {
      throw new Test262Error(message + ': expected ' + Ctor.name + ', got ' + e);
    }
    return;
  }
  throw new Test262Error(message + ': expected ' + Ctor.name + ' to be thrown');
}

function cleanupIndex(proto, key) {
  delete proto[key];
}

// Overwrite, append, hole-fill and length bookkeeping.
(function () {
  var a = [1, 2, 3];
  for (var i = 0; i < 3; i++) store(a, i, i * 10);
  assertSame(a.join(), '0,10,20', 'in-bounds overwrite');
  assertSame(a.length, 3, 'overwrite leaves length alone');

  store(a, 3, 30);
  assertSame(a.length, 4, 'append grows length');
  assertSame(load(a, 3), 30, 'appended value readable');

  var h = new Array(4);
  store(h, 2, 'x');
  assertSame(h.length, 4, 'hole-fill inside length leaves length alone');
  assertSame(2 in h, true, 'filled hole is present');
  assertSame(1 in h, false, 'untouched hole stays a hole');
  assertSame(load(h, 1), undefined, 'hole reads undefined');

  var s = [];
  store(s, 0, 'a');
  store(s, 1, 'b');
  assertSame(s.length, 2, 'append to empty array');
  store(s, 5, 'f');
  assertSame(s.length, 6, 'sparse write grows length');
  assertSame(3 in s, false, 'gap is not materialised');

  var z = [0];
  store(z, -0, 'zero');
  assertSame(z[0], 'zero', '-0 addresses element zero');
  assertSame(z.length, 1, '-0 does not append');
})();

// Present undefined is not a hole.
(function () {
  var a = [1, 2, 3];
  store(a, 1, undefined);
  assertSame(1 in a, true, 'stored undefined is present');
  assertSame(a.hasOwnProperty(1), true, 'stored undefined is an own property');
  assertSame(Object.keys(a).join(), '0,1,2', 'stored undefined is enumerated');
  store(a, 1, 7);
  assertSame(a[1], 7, 'overwrite of present undefined');
  assertSame(Object.getOwnPropertyDescriptor(a, 1).writable, true, 'attributes kept');

  var b = [1, , 3];
  assertSame(load(b, 1), undefined, 'hole read');
  assertSame(1 in b, false, 'hole is absent');
  store(b, 1, undefined);
  assertSame(1 in b, true, 'writing undefined into a hole makes it present');

  var c = [];
  store(c, 0, undefined);
  assertSame(c.length, 1, 'appended undefined grows length');
  assertSame(0 in c, true, 'appended undefined is present');
})();

// Reads honour accessors and non-default attributes defined on an element.
(function () {
  var a = [1, 2, 3];
  var calls = 0;
  Object.defineProperty(a, 1, { get: function () { calls++; return 'got'; }, configurable: true });
  assertSame(load(a, 1), 'got', 'getter on element');
  assertSame(calls, 1, 'getter ran once');
  store(a, 1, 'ignored');
  assertSame(load(a, 1), 'got', 'write to getter-only element is ignored');
  assertThrows(TypeError, function () { storeStrict(a, 1, 'x'); }, 'strict write to getter-only element');

  var b = [1, 2, 3];
  var seen;
  Object.defineProperty(b, 2, { set: function (v) { seen = v; }, configurable: true });
  store(b, 2, 'via setter');
  assertSame(seen, 'via setter', 'setter on element runs');
  assertSame(load(b, 2), undefined, 'setter-only element reads undefined');

  var c = [1, 2, 3];
  Object.defineProperty(c, 0, { value: 'ro', writable: false, enumerable: true, configurable: true });
  store(c, 0, 'rw');
  assertSame(load(c, 0), 'ro', 'non-writable element is not overwritten');
  assertThrows(TypeError, function () { storeStrict(c, 0, 'x'); }, 'strict write to non-writable element');
  store(c, 1, 'ok');
  assertSame(load(c, 1), 'ok', 'sibling elements stay writable');
})();

// Integrity levels.
(function () {
  var f = Object.freeze([1, 2, 3]);
  store(f, 0, 'x');
  store(f, 3, 'y');
  assertSame(f.join(), '1,2,3', 'sloppy writes to a frozen array are ignored');
  assertSame(f.length, 3, 'frozen length unchanged');
  assertThrows(TypeError, function () { storeStrict(f, 0, 'x'); }, 'strict overwrite of frozen element');
  assertThrows(TypeError, function () { storeStrict(f, 3, 'x'); }, 'strict append to frozen array');

  var s = Object.seal([1, 2, 3]);
  store(s, 1, 'ok');
  assertSame(s[1], 'ok', 'sealed elements stay writable');
  store(s, 3, 'no');
  assertSame(s.length, 3, 'sealed array cannot grow');
  assertThrows(TypeError, function () { storeStrict(s, 3, 'x'); }, 'strict append to sealed array');

  var p = Object.preventExtensions([1, , 3]);
  store(p, 0, 'ok');
  assertSame(p[0], 'ok', 'non-extensible overwrite of a present element');
  store(p, 1, 'no');
  assertSame(1 in p, false, 'non-extensible array cannot fill a hole');
  store(p, 3, 'no');
  assertSame(p.length, 3, 'non-extensible array cannot grow');
  assertThrows(TypeError, function () { storeStrict(p, 1, 'x'); }, 'strict hole-fill on non-extensible array');

  var l = [1, 2];
  Object.defineProperty(l, 'length', { writable: false });
  store(l, 0, 'ok');
  assertSame(l[0], 'ok', 'overwrite with read-only length');
  store(l, 2, 'no');
  assertSame(l.length, 2, 'read-only length blocks append');
  assertSame(2 in l, false, 'no element created past read-only length');
  assertThrows(TypeError, function () { storeStrict(l, 2, 'x'); }, 'strict append past read-only length');
})();

// Inherited setters and read-only properties apply to holes and appends. Each
// prototype is polluted in a fresh realm because an Array or Object prototype
// that has ever held an index key keeps the element fast paths off for good.
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
    assertSame(log, 'appended;', name + '.prototype setter intercepts append');
    assertSame(a.length, 1, name + '.prototype setter: append creates nothing');
    assertSame(load(a, 1), 'inherited', name + '.prototype getter read');

    var h = new g.Array(3);
    store(h, 1, 'hole');
    assertSame(log, 'appended;hole;', name + '.prototype setter intercepts hole fill');
    assertSame(h.hasOwnProperty(1), false, name + '.prototype setter: hole not materialised');

    var o = g.Array.of(0, 5);
    store(o, 1, 'own');
    assertSame(o[1], 'own', 'own element wins over inherited accessor');
    assertSame(count, 2, 'own overwrite does not call the setter');
    delete proto[1];

    Object.defineProperty(proto, 2, { value: 'ro', writable: false, configurable: true });
    var r = g.Array.of(0, 1);
    store(r, 2, 'x');
    assertSame(r.length, 2, name + '.prototype read-only index blocks append');
    assertSame(load(r, 2), 'ro', name + '.prototype data property read');
    assertThrows(TypeError, function () { storeStrict(r, 2, 'x'); }, 'strict append over inherited read-only');
    delete proto[2];

    Object.defineProperty(proto, 0, { value: 'shadow', writable: true, configurable: true });
    var d = new g.Array();
    store(d, 0, 'mine');
    assertSame(Object.getOwnPropertyDescriptor(d, 0).value, 'mine', 'writable inherited data property: own element created');
    assertSame(d.length, 1, 'length after create');
    delete proto[0];

    var after = new g.Array();
    store(after, 0, 'again');
    assertSame(after.length, 1, 'append works after prototype cleanup');
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
  assertSame(trapped.join(), '1=p', 'proxy prototype set trap runs for append');
  assertSame(a[1], 'p', 'Reflect.set with receiver created the element');
  store(a, 0, 'own');
  assertSame(trapped.length, 1, 'overwrite of an own element does not consult the proxy');

  var b = [0];
  Object.setPrototypeOf(b, Object.create(proto));
  store(b, 1, 'deep');
  assertSame(trapped.length, 2, 'proxy two links up the chain');

  var holey = new Array(3);
  Object.setPrototypeOf(holey, proto);
  store(holey, 1, 'h');
  assertSame(trapped[2], '1=h', 'proxy prototype set trap runs for hole fill');
})();

// Receivers that are not plain Arrays on the chain.
(function () {
  var str = new String('abc');
  var a = [];
  Object.setPrototypeOf(a, str);
  store(a, 0, 'z');
  assertSame(a.length, 0, 'String wrapper prototype: read-only character blocks the write');
  assertSame(load(a, 0), 'a', 'String wrapper prototype: character visible');
  assertThrows(TypeError, function () { storeStrict(a, 1, 'x'); }, 'String wrapper prototype: strict write rejected');

  var boxed = [];
  Object.setPrototypeOf(boxed, Object('abc'));
  store(boxed, 0, 'z');
  assertSame(boxed.length, 0, 'boxed string prototype: read-only character blocks the write');
  assertThrows(TypeError, function () { storeStrict(boxed, 0, 'x'); }, 'boxed string prototype: strict write rejected');

  var ta = new Uint8Array(2);
  var b = [];
  Object.setPrototypeOf(b, ta);
  store(b, 5, 'x');
  assertSame(b.length, 0, 'TypedArray prototype swallows an out-of-range write');

  class Sub extends Array {}
  var s = new Sub();
  store(s, 0, 'a');
  store(s, 1, 'b');
  assertSame(s.length, 2, 'Array subclass appends');
  assertSame(s instanceof Sub, true, 'subclass identity kept');
  store(s, 0, 'c');
  assertSame(s.join(), 'c,b', 'Array subclass overwrite');
})();

// Named properties alongside elements, and index-like names.
(function () {
  var a = [1, 2, 3];
  a.foo = 'bar';
  a['01'] = 'octal-like';
  a['1.5'] = 'fraction';
  assertSame(load(a, 1), 2, 'named properties do not disturb element reads');
  store(a, 1, 'two');
  assertSame(a[1], 'two', 'overwrite with named properties present');
  store(a, 3, 'four');
  assertSame(a.length, 4, 'append with named properties present');
  assertSame(a['01'], 'octal-like', 'non-canonical index name untouched');
  assertSame(load(a, 1.5), 'fraction', 'fractional key reads the named property');

  var b = [];
  b[4294967295] = 'not an index';
  assertSame(b.length, 0, '2^32-1 is not an array index');
  b[4294967294] = 'last index';
  assertSame(b.length, 4294967295, 'largest index sets length to 2^32-1');
  store(b, 0, 'first');
  assertSame(b[0], 'first', 'element zero of a huge sparse array');
})();

// Non-canonical numeric strings are ordinary property names, not elements.
(function () {
  var a = [1, 2, 3];
  assertSame(a['01'], undefined, 'leading zero read');
  assertSame(a['+1'], undefined, 'plus sign read');
  assertSame(a['1.0'], undefined, 'trailing zero read');
  assertSame('01' in a, false, 'leading zero is not present');
  assertSame(a.hasOwnProperty('+1'), false, 'plus sign is not an own property');
  assertSame(delete a['01'], true, 'deleting an absent non-canonical name succeeds');
  assertSame(a[1], 2, 'delete of "01" leaves element one alone');
  assertSame(a.length, 3, 'delete of "01" leaves length alone');
  a['01'] = 'named';
  assertSame(a[1], 2, 'write to "01" leaves element one alone');
  assertSame(a['01'], 'named', 'write to "01" is an ordinary property');
  assertSame(Object.keys(a).join(), '0,1,2,01', 'keys list the element indices first');
  a['04'] = 'named4';
  assertSame(a.length, 3, 'write to "04" does not grow the array');
  assertSame(4 in a, false, 'write to "04" does not create element four');
  assertSame(Reflect.deleteProperty(a, '01'), true, 'Reflect.deleteProperty of "01"');
  assertSame(a[1], 2, 'Reflect.deleteProperty of "01" leaves element one alone');
  assertSame(store(Array.of(7, 8, 9), '02', 'x')[2], 9, 'created array: write to "02"');
})();

// Non-number keys and out-of-range numbers use the generic path unchanged.
(function () {
  var a = [1, 2, 3];
  assertSame(load(a, '1'), 2, 'string key read');
  assertSame(load(a, 1.0), 2, 'integral double read');
  assertSame(load(a, 3), undefined, 'one past the end');
  assertSame(load(a, -1), undefined, 'negative index read');
  assertSame(load(a, NaN), undefined, 'NaN read');
  store(a, -1, 'neg');
  assertSame(a[-1], 'neg', 'negative index creates a named property');
  assertSame(a.length, 3, 'negative index does not touch length');
  store(a, 1.5, 'frac');
  assertSame(a['1.5'], 'frac', 'fractional index creates a named property');
  store(a, '2', 'str');
  assertSame(a[2], 'str', 'string key overwrite');
})();

// A stored object keeps its identity and survives a collection.
(function () {
  var a = new Array(64);
  for (var i = 0; i < 64; i++) store(a, i, { n: i });
  if (typeof $262 !== 'undefined' && $262.gc) $262.gc();
  for (var j = 0; j < 64; j++) assertSame(load(a, j).n, j, 'object element ' + j);
})();
