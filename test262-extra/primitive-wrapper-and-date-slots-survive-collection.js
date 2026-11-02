/*---
description: >
  Wrapper objects and Dates keep their [[BooleanData]], [[NumberData]],
  [[StringData]], [[SymbolData]], [[BigIntData]] and [[DateValue]] slots across
  collections, and a FinalizationRegistry keeps its cleanup callback, when the
  only reference to each value is the object that wraps it.
esid: sec-properties-of-the-boolean-instances
info: |
  Boolean, Number, String, Symbol, BigInt and Date instances carry one internal
  slot holding the primitive; the collector must keep that primitive alive
  while the object is reachable, and ordinary objects carry no such slot. The
  wrapped strings, symbols and bigints are built at run time so that nothing
  else references them, and the slot is read back after collections.
features: [Symbol, BigInt, FinalizationRegistry, host-gc-required]
includes: [compareArray.js]
---*/

function build(i) {
  return {
    str: new String("s" + i + "-" + "x".repeat(i % 7)),
    num: new Number(i + 0.5),
    bool: new Boolean(i % 2 === 0),
    sym: Object(Symbol("sym" + i)),
    big: Object(BigInt(i) * 1000000007n),
    date: new Date(1700000000000 + i),
    plain: { i: i },
  };
}

var kept = [];
for (var i = 0; i < 40; i++) {
  kept.push(build(i));
}

var churn;
for (var round = 0; round < 3; round++) {
  churn = [];
  for (var j = 0; j < 2000; j++) {
    churn.push({ a: j, s: new String("t" + j) });
  }
  $262.gc();
}
churn = undefined;
$262.gc();

for (var i = 0; i < 40; i++) {
  var o = kept[i];
  var text = "s" + i + "-" + "x".repeat(i % 7);
  assert.sameValue(o.str.valueOf(), text, "String slot " + i);
  assert.sameValue(o.str.length, text.length, "String length " + i);
  assert.sameValue(o.num.valueOf(), i + 0.5, "Number slot " + i);
  assert.sameValue(o.bool.valueOf(), i % 2 === 0, "Boolean slot " + i);
  assert.sameValue(o.sym.valueOf().description, "sym" + i, "Symbol slot " + i);
  assert.sameValue(typeof o.sym.valueOf(), "symbol");
  assert.sameValue(o.big.valueOf(), BigInt(i) * 1000000007n, "BigInt slot " + i);
  assert.sameValue(o.date.getTime(), 1700000000000 + i, "Date slot " + i);
  assert.sameValue(o.plain.i, i);
  assert.sameValue(Object.prototype.toString.call(o.str), "[object String]");
  assert.sameValue(Object.prototype.toString.call(o.date), "[object Date]");
  assert.sameValue(Object.prototype.toString.call(o.plain), "[object Object]");
}

assert.sameValue(
  JSON.stringify([kept[3].str, kept[3].num, kept[3].bool]),
  '["s3-xxx",3.5,false]',
  "JSON.stringify unwraps wrapper objects"
);
assert.sameValue(JSON.stringify(kept[5].date), '"2023-11-14T22:13:20.005Z"');
assert.compareArray(Object.getOwnPropertyNames(kept[2].str), ["0", "1", "2", "3", "4", "length"]);

var plain = {};
assert.throws(TypeError, function () {
  Number.prototype.valueOf.call(plain);
});
assert.throws(TypeError, function () {
  Date.prototype.getTime.call(plain);
});
assert.throws(TypeError, function () {
  String.prototype.valueOf.call(plain);
});
assert.throws(TypeError, function () {
  Boolean.prototype.valueOf.call({});
});

var registry = new FinalizationRegistry(function () {});
$262.gc();
$262.gc();
var target = {};
registry.register(target, "held", target);
assert.sameValue(registry.unregister(target), true, "the registry still accepts cells after collections");
assert.sameValue(registry.unregister(target), false);
