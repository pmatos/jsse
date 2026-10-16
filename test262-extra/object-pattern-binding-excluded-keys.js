// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-destructuring-binding-patterns-runtime-semantics-bindinginitialization
description: >
  Object binding patterns evaluate computed keys in order and read each
  property exactly once whether or not a rest element follows, and a rest
  element excludes every key bound before it: shorthand, identifier, string,
  numeric and computed names.
---*/

var log = [];
var source = {
  get a() { log.push("get a"); return 1; },
  get b() { log.push("get b"); return 2; },
  get c() { log.push("get c"); return 3; },
  get 7() { log.push("get 7"); return 4; },
  get d() { log.push("get d"); return 5; }
};

function key(name) {
  log.push("key " + name);
  return name;
}

{
  const { a, b } = source;
  assert.sameValue(a + b, 3);
  assert.sameValue(log.join(), "get a,get b", "shorthand without rest");
}

log.length = 0;
{
  const { [key("a")]: x, b: y, ["c"]: z } = source;
  assert.sameValue(x + y + z, 6);
  assert.sameValue(log.join(), "key a,get a,get b,get c", "computed and renamed without rest");
}

log.length = 0;
{
  const { a, [key("b")]: renamed, 7: seven, "d": dee, ...others } = source;
  assert.sameValue(renamed, 2);
  assert.sameValue(seven, 4);
  assert.sameValue(dee, 5);
  assert.sameValue(Object.keys(others).join(), "c", "rest excludes shorthand, computed, numeric and string keys");
  assert.sameValue(log.join(), "get a,key b,get b,get 7,get d,get c", "rest reads the remaining key last");
}

{
  let a, rest;
  ({ a, ...rest } = { a: 1, b: 2, c: 3 });
  assert.sameValue(Object.keys(rest).join(), "b,c");
}

{
  const { x: X, ...q } = { x: 1, y: 2 };
  assert.sameValue(X, 1);
  assert.sameValue(JSON.stringify(q), '{"y":2}');
}

{
  const { 0: z, ...w } = { 0: "zero", 1: "one" };
  assert.sameValue(z, "zero");
  assert.sameValue(JSON.stringify(w), '{"1":"one"}');
}

{
  var symbol = Symbol("s");
  const { [symbol]: viaSymbol, ...noSymbol } = { [symbol]: 1, other: 2 };
  assert.sameValue(viaSymbol, 1);
  assert.sameValue(Object.getOwnPropertySymbols(noSymbol).length, 0, "symbol key excluded");
  assert.sameValue(noSymbol.other, 2);
}

assert.throws(TypeError, function () {
  const {} = null;
}, "const {} = null");
assert.throws(TypeError, function () {
  const { a } = undefined;
}, "const { a } = undefined");
assert.throws(TypeError, function () {
  const { ...r } = null;
}, "const { ...r } = null");

{
  const {} = 1;
  const { length } = "abc";
  assert.sameValue(length, 3, "primitive source");
}

{
  var calls = 0;
  const { a = (calls++, 10), b = (calls++, 20) } = { a: 1 };
  assert.sameValue(a, 1);
  assert.sameValue(b, 20);
  assert.sameValue(calls, 1, "defaults evaluate only for undefined");
}

{
  var traps = [];
  var proxy = new Proxy({ p: 1, q: 2 }, {
    get: function (target, name) {
      traps.push("get " + String(name));
      return target[name];
    },
    ownKeys: function (target) {
      traps.push("ownKeys");
      return Reflect.ownKeys(target);
    },
    getOwnPropertyDescriptor: function (target, name) {
      traps.push("gopd " + String(name));
      return Reflect.getOwnPropertyDescriptor(target, name);
    }
  });
  const { p, q } = proxy;
  assert.sameValue(p + q, 3);
  assert.sameValue(traps.join(), "get p,get q", "no rest: only the two gets");
  traps.length = 0;
  const { p: p2, ...remaining } = proxy;
  assert.sameValue(remaining.q, 2);
  assert.sameValue(traps.join(), "get p,ownKeys,gopd q,get q", "rest: gets, then ownKeys and the remaining key");
}

(function () {
  var local = { m: 1, n: 2 };
  var { m, ...tail } = local;
  assert.sameValue(m, 1);
  assert.sameValue(Object.keys(tail).join(), "n", "var declaration with rest");
  var { m: m2, n } = local;
  assert.sameValue(m2 + n, 3, "var declaration without rest");
})();
