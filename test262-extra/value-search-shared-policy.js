/*---
description: Array and TypedArray value searches preserve preparation, range, and access policy
esid: sec-array.prototype.indexof
info: |
  Array.prototype.indexOf/lastIndexOf use HasProperty and IsStrictlyEqual;
  includes uses Get and SameValueZero. The corresponding TypedArray methods
  validate before capturing length, but read live elements after fromIndex
  coercion. All six return early for an empty range before that coercion.
includes: [compareArray.js]
features: [TypedArray, resizable-arraybuffer, BigInt]
---*/

for (var make of [
  function (values) { return values; },
  function (values) { return new Float64Array(values); }
]) {
  var receiver = make([1, NaN, -0, 1]);
  assert.sameValue(receiver.indexOf(1), 0);
  assert.sameValue(receiver.lastIndexOf(1), 3);
  assert.sameValue(receiver.lastIndexOf(1, undefined), 0);
  assert.sameValue(receiver.indexOf(NaN), -1);
  assert.sameValue(receiver.lastIndexOf(NaN), -1);
  assert.sameValue(receiver.includes(NaN), true);
  assert.sameValue(receiver.indexOf(0), 2);
  assert.sameValue(receiver.lastIndexOf(+0), 2);
  assert.sameValue(receiver.includes(+0), true);
  assert.sameValue(receiver.indexOf(1, Infinity), -1);
  assert.sameValue(receiver.lastIndexOf(1, Infinity), 3);
  assert.sameValue(receiver.includes(1, Infinity), false);
  assert.sameValue(receiver.indexOf(1, -Infinity), 0);
  assert.sameValue(receiver.lastIndexOf(1, -Infinity), -1);
  assert.sameValue(receiver.includes(1, -Infinity), true);
  assert.sameValue(receiver.indexOf(1, -1.9), 3);
  assert.sameValue(receiver.lastIndexOf(1, -5), -1);
  assert.sameValue(receiver.includes(1, -1), true);
  for (var name of ['indexOf', 'lastIndexOf', 'includes']) {
    var from = { valueOf: function () { throw new Test262Error('empty coercion'); } };
    assert.sameValue(make([])[name](1, from), name === 'includes' ? false : -1);
    var calls = 0;
    from = { valueOf: function () { ++calls; return 1; } };
    receiver[name](1, from);
    assert.sameValue(calls, 1);
    assert.sameValue(receiver[name]('1'), name === 'includes' ? false : -1);
    assert.throws(Test262Error, function () {
      receiver[name](1, { valueOf: function () { throw new Test262Error(); } });
    });
  }
}

// Dense prefixes resume at a hole; inherited indices remain observable.
var sparse = [1, , 3, 1];
var proto = Object.create(Array.prototype);
proto[1] = 2;
Object.setPrototypeOf(sparse, proto);
assert.sameValue(sparse.indexOf(2), 1);
assert.sameValue(sparse.lastIndexOf(2), 1);
assert.sameValue(sparse.includes(2), true);
delete proto[1];
assert.sameValue(sparse.indexOf(undefined), -1);
assert.sameValue(sparse.lastIndexOf(undefined), -1);
assert.sameValue(sparse.includes(undefined), true);
assert.sameValue([1, undefined, 3].indexOf(undefined), 1);
assert.sameValue([1, undefined, 3].lastIndexOf(undefined), 1);
var symbol = Symbol();
var object = {};
assert.sameValue([symbol, object].indexOf(symbol), 0);
assert.sameValue([symbol, object].lastIndexOf(object), 1);
assert.sameValue([object].includes({}), false);
for (var big of [[1n, 2n, 1n], new BigInt64Array([1n, 2n, 1n])]) {
  assert.sameValue(big.indexOf(1n), 0);
  assert.sameValue(big.lastIndexOf(1n), 2);
  assert.sameValue(big.includes(1n), true);
  assert.sameValue(big.includes(1), false);
}

// The initially captured length, HasProperty/Get order, and early stop are fixed.
for (var name of ['indexOf', 'lastIndexOf', 'includes']) {
  var log = [];
  var source = { 0: 'x', 2: 'x', length: 3 };
  var proxy = new Proxy(source, {
    get: function (target, key) { log.push('get ' + key); return target[key]; },
    has: function (target, key) { log.push('has ' + key); return key in target; }
  });
  var from = { valueOf: function () { log.push('from'); source.length = 0; return name === 'lastIndexOf' ? 2 : 0; } };
  assert.sameValue(Array.prototype[name].call(proxy, 'x', from), name === 'includes' ? true : name === 'lastIndexOf' ? 2 : 0);
  assert.compareArray(log, name === 'includes'
    ? ['get length', 'from', 'get 0']
    : ['get length', 'from', 'has ' + (name === 'lastIndexOf' ? 2 : 0), 'get ' + (name === 'lastIndexOf' ? 2 : 0)]);
  log.length = 0;
  source.length = 3;
  assert.sameValue(Array.prototype[name].call(proxy, undefined), name === 'includes' ? true : -1);
  assert.compareArray(log, name === 'includes'
    ? ['get length', 'get 0', 'get 1']
    : name === 'lastIndexOf'
      ? ['get length', 'has 2', 'get 2', 'has 1', 'has 0', 'get 0']
      : ['get length', 'has 0', 'get 0', 'has 1', 'has 2', 'get 2']);
  assert.throws(Test262Error, function () {
    Array.prototype[name].call({ get length() { throw new Test262Error(); } }, 1);
  });
  assert.throws(Test262Error, function () {
    Array.prototype[name].call({ length: 1, get 0() { throw new Test262Error(); } }, 1);
  });
}

// Captured TypedArray range survives detachment/shrink during coercion.
for (var name of ['indexOf', 'lastIndexOf', 'includes']) {
  for (var mutate of [
    function (buffer) { $262.detachArrayBuffer(buffer); },
    function (buffer) { buffer.resize(0); }
  ]) {
    var buffer = new ArrayBuffer(3, { maxByteLength: 6 });
    var typed = new Uint8Array(buffer);
    typed.set([1, 2, 1]);
    var from = { valueOf: function () { mutate(buffer); return name === 'lastIndexOf' ? 2 : 0; } };
    assert.sameValue(typed[name](undefined, from), name === 'includes' ? true : -1);
  }
  var method = Uint8Array.prototype[name];
  assert.throws(TypeError, function () { method.call([], 1); });
  var detached = new Uint8Array(0);
  $262.detachArrayBuffer(detached.buffer);
  assert.throws(TypeError, function () { method.call(detached, 1); });
  var buffer = new ArrayBuffer(3, { maxByteLength: 6 });
  var fixed = new Uint8Array(buffer, 0, 3);
  buffer.resize(1);
  assert.throws(TypeError, function () { method.call(fixed, 1); });
}
