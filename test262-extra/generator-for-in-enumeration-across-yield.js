/*---
description: >
  A generator for-in loop enumerates lazily with respect to deletion and
  reports proxy trap failures from the resumed generator.
esid: sec-enumerate-object-properties
info: |
  EnumerateObjectProperties: a property that is deleted before it is
  processed is not visited, no property name is visited more than once, and
  abrupt completions from the target's internal methods propagate.
includes: [compareArray.js]
features: [generators, Proxy]
---*/

function* walk(o, log) {
  for (var k in o) {
    log.push(k);
    yield k;
  }
}

var target = { a: 1, b: 2, c: 3 };
var log = [];
var it = walk(target, log);
assert.sameValue(it.next().value, 'a', 'first key');
delete target.b;
assert.sameValue(it.next().value, 'c', 'a key deleted while suspended is skipped');
assert.sameValue(it.next().done, true, 'enumeration ends');
assert.compareArray(log, ['a', 'c'], 'deleted key never bound');

var seen = {};
var dup = Object.create({ x: 1, y: 2 });
dup.x = 3;
dup.z = 4;
[...walk(dup, [])].forEach(function (k) {
  assert.sameValue(seen[k], undefined, 'no duplicate: ' + k);
  seen[k] = true;
});
assert.compareArray(Object.keys(seen).sort(), ['x', 'y', 'z'], 'each visible key visited once');

var trapLog = [];
var proxy = new Proxy({ p: 1, q: 2 }, {
  ownKeys: function (t) {
    trapLog.push('ownKeys');
    return Reflect.ownKeys(t);
  },
  getOwnPropertyDescriptor: function (t, k) {
    trapLog.push('gopd:' + String(k));
    return Reflect.getOwnPropertyDescriptor(t, k);
  },
});
assert.compareArray([...walk(proxy, [])], ['p', 'q'], 'proxy keys enumerated');
assert.sameValue(trapLog[0], 'ownKeys', 'ownKeys consulted before the first key is bound');

var throwing = new Proxy({ p: 1 }, {
  ownKeys: function () {
    throw new Test262Error('ownKeys failure');
  },
});
it = walk(throwing, []);
assert.throws(Test262Error, function () {
  it.next();
}, 'proxy trap failure surfaces from next()');
assert.sameValue(it.next().done, true, 'generator is completed afterwards');

function* caught(o) {
  try {
    for (var k in o) yield k;
  } catch (e) {
    yield e.message;
  }
}
assert.compareArray([...caught(throwing)], ['ownKeys failure'], 'trap failure caught by a try around the loop');
