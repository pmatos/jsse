/*---
description: >
  A named read `obj.name`, a method load `obj.name(...)` and an update
  `obj.name++` (the bytecode VM's `GetProp`, `LoadMethod` and `UpdateProp`)
  behave as OrdinaryGet / [[Set]] specify: inherited accessors and Proxies are
  consulted with the original receiver, a name added or deleted after earlier
  reads is seen at once, and primitives, exotic objects and non-writable or
  accessor properties keep their semantics.
esid: sec-ordinary-object-internal-methods-and-internal-slots-get-p-receiver
features: [Proxy, TypedArray, Reflect]
flags: [noStrict]
---*/

function getX(o) { return o.x; }
function callM(o) { return o.m(); }
function incX(o) { return o.x++; }
function preDecX(o) { return --o.x; }

// Own, inherited, absent: and the chain changing under a hot site.
(function () {
  var proto = { x: "proto" };
  var o = Object.create(proto);
  for (var i = 0; i < 3; i++) assert.sameValue(getX(o), "proto", "inherited data property");
  o.x = "own";
  assert.sameValue(getX(o), "own", "own property added after reads shadows the prototype");
  delete o.x;
  assert.sameValue(getX(o), "proto", "deleted: the prototype shows through");
  delete proto.x;
  assert.sameValue(getX(o), undefined, "absent everywhere");
  Object.prototype.x = "from Object.prototype";
  try {
    assert.sameValue(getX(o), "from Object.prototype", "added late to Object.prototype");
  } finally {
    delete Object.prototype.x;
  }
  assert.sameValue(getX(o), undefined, "and gone again");
  Object.setPrototypeOf(o, { x: "new proto" });
  assert.sameValue(getX(o), "new proto", "prototype replaced");
  Object.setPrototypeOf(o, null);
  assert.sameValue(getX(o), undefined, "null prototype");
})();

// Accessors keep their receiver.
(function () {
  var recv;
  var proto = { get x() { recv = this; return "accessor"; } };
  var o = Object.create(proto);
  assert.sameValue(getX(o), "accessor", "inherited getter");
  assert.sameValue(recv, o, "receiver is the original object");
  var own = { get x() { return this === own; } };
  assert.sameValue(getX(own), true, "own getter");
  var setterOnly = { set x(v) {} };
  assert.sameValue(getX(setterOnly), undefined, "setter-only reads undefined");
  var base = Object.create(Object.create({ get x() { return this.tag; } }));
  base.tag = "deep";
  assert.sameValue(getX(base), "deep", "getter two levels up sees the receiver");
})();

// Proxies at the receiver and on the chain.
(function () {
  var log = [];
  var handler = { get: function (t, k, r) { log.push(String(k), r === proxy || r === child); return "trapped"; } };
  var proxy = new Proxy({ x: "target" }, handler);
  assert.sameValue(getX(proxy), "trapped", "Proxy receiver");
  var child = Object.create(proxy);
  assert.sameValue(getX(child), "trapped", "Proxy on the chain");
  assert.sameValue(log.join(), "x,true,x,true", "get trap receives the original receiver");
  var noTrap = Object.create(new Proxy({ x: "via target" }, {}));
  assert.sameValue(getX(noTrap), "via target", "trap-less Proxy forwards");
  var shadowed = Object.create(proxy);
  shadowed.x = "own";
  log.length = 0;
  assert.sameValue(getX(shadowed), "own", "own property wins over the Proxy");
  assert.sameValue(log.length, 0, "trap not called");
})();

// Exotic receivers.
(function () {
  function getLength(o) { return o.length; }
  assert.sameValue(getLength([1, 2, 3]), 3, "Array length");
  assert.sameValue(getLength("abc"), 3, "string primitive length");
  assert.sameValue(getLength(new String("ab")), 2, "String wrapper length");
  assert.sameValue(getLength(Object.create(new String("abcd"))), 4, "String wrapper on the chain");
  assert.sameValue(getLength(new Uint8Array(5)), 5, "TypedArray length is a prototype accessor");
  assert.sameValue(getLength((function (a, b) { return arguments; })(1, 2)), 2, "arguments length");
  assert.sameValue(getLength(function (a, b, c) {}), 3, "function length");
  function getFirst(o) { return o.first; }
  var ta = new Uint8Array(2);
  Uint8Array.prototype.first = "tap";
  try {
    assert.sameValue(getFirst(ta), "tap", "named read on a TypedArray reaches the prototype");
  } finally {
    delete Uint8Array.prototype.first;
  }
  function getNaN(o) { return o.NaN; }
  assert.sameValue(getNaN(ta), undefined, "canonical numeric name on a TypedArray never reaches the chain");
  Object.prototype.NaN = "inherited";
  try {
    assert.sameValue(getNaN(ta), undefined, "even when the chain has it");
    assert.sameValue(getNaN({}), "inherited", "while a plain object sees it");
  } finally {
    delete Object.prototype.NaN;
  }
  function getZero(o) { return o["0"]; }
  assert.sameValue(getZero([7]), 7, "index-like name on an Array");
  assert.sameValue(getZero("s"), "s", "index-like name on a string");
  assert.sameValue(getZero(new Uint8Array([9])), 9, "index-like name on a TypedArray");
})();

// Method calls: the receiver is the base, an own or inherited method is found.
(function () {
  var o = { v: 1, m: function () { return this.v; } };
  assert.sameValue(callM(o), 1, "own method");
  var child = Object.create(o);
  child.v = 2;
  assert.sameValue(callM(child), 2, "inherited method called on the child");
  child.m = function () { return "shadow"; };
  assert.sameValue(callM(child), "shadow", "own method added later");
  delete child.m;
  assert.sameValue(callM(child), 2, "deleted again");
  assert.throws(TypeError, function () { callM({}); }, "missing method");
  assert.throws(TypeError, function () { callM({ m: 1 }); }, "non-callable");
  assert.sameValue(callM(Object.create({ get m() { return function () { return "getter"; }; } })), "getter", "accessor method");
  assert.sameValue(callM("str".constructor === String ? { m: String.prototype.toString.bind("p") } : null), "p", "bound");
  var proxy = new Proxy({}, { get: function (t, k) { return function () { return "proxy " + String(k); }; } });
  assert.sameValue(callM(proxy), "proxy m", "Proxy method");
})();

// Update expressions.
(function () {
  var o = { x: 1 };
  assert.sameValue(incX(o), 1, "postfix result is the old value");
  assert.sameValue(o.x, 2, "stored");
  assert.sameValue(preDecX(o), 1, "prefix result is the new value");
  var s = { x: "5" };
  assert.sameValue(incX(s), 5, "postfix converts to Number");
  assert.sameValue(s.x, 6, "string operand stored as Number");
  var big = { x: 1n };
  incX(big);
  assert.sameValue(big.x, 2n, "BigInt");
  var u = {};
  incX(u);
  assert.sameValue(u.x, NaN, "absent property becomes NaN");
  var ro = Object.defineProperty({}, "x", { value: 1, writable: false });
  incX(ro);
  assert.sameValue(ro.x, 1, "read-only property: ignored");
  assert.throws(TypeError, function () { "use strict"; ro.x++; }, "strict");
  var frozen = Object.freeze({ x: 1 });
  incX(frozen);
  assert.sameValue(frozen.x, 1, "frozen");
  var log = [];
  var acc = { get x() { log.push("get"); return 1; }, set x(v) { log.push("set" + v); } };
  assert.sameValue(incX(acc), 1, "accessor result");
  assert.sameValue(log.join(), "get,set2", "getter then setter");
  var inherited = Object.create({ x: 10 });
  assert.sameValue(incX(inherited), 10, "inherited value");
  assert.sameValue(inherited.hasOwnProperty("x"), true, "own property created by the update");
  assert.sameValue(inherited.x, 11, "created with the new value");
  assert.sameValue(Object.getPrototypeOf(inherited).x, 10, "prototype untouched");
  var order = [];
  var objWithValueOf = { x: { valueOf: function () { order.push("valueOf"); return 3; } } };
  assert.sameValue(incX(objWithValueOf), 3, "object operand converted");
  assert.sameValue(order.join(), "valueOf", "valueOf once");
  var arr = [1, 2];
  function incLength(a) { return a.length++; }
  assert.sameValue(incLength(arr), 2, "Array length old value");
  assert.sameValue(arr.length, 3, "Array length incremented through ArraySetLength");
  var holes = [1, 2, 3];
  function decLength(a) { return --a.length; }
  assert.sameValue(decLength(holes), 2, "Array length decremented");
  assert.sameValue(holes[2], undefined, "element removed");
  var wrapper = new String("ab");
  function incWrapperLength(o) { o.length++; }
  incWrapperLength(wrapper);
  assert.sameValue(wrapper.length, 2, "String length is read-only");
  var counts = { n: 0 };
  for (var i = 0; i < 1000; i++) counts.n++;
  assert.sameValue(counts.n, 1000, "loop");
  var nan = { x: NaN };
  incX(nan);
  assert.sameValue(nan.x, NaN, "NaN stays NaN");
  var neg = { x: -0 };
  assert.sameValue(Object.is(incX(neg), -0), true, "postfix -0 old value");
  assert.sameValue(neg.x, 1, "-0 + 1");
  var proxy = new Proxy({ x: 1 }, {
    get: function (t, k, r) { log.push("pget"); return Reflect.get(t, k, r); },
    set: function (t, k, v, r) { log.push("pset" + v); return Reflect.set(t, k, v, r); },
  });
  log.length = 0;
  incX(proxy);
  assert.sameValue(log.join(), "pget,pset2", "Proxy get then set traps");
})();

// A young object stored into an old object by a named store stays reachable.
(function () {
  var holder = { slot: null, other: null };
  var keep = [];
  $262.gc();
  $262.gc();
  function churn() {
    var junk;
    for (var k = 0; k < 64; k++) junk = { k: k, next: junk };
    return junk;
  }
  function store(h, v) { h.slot = v; }
  function storeNew(h, v) { h.late = v; }
  for (var i = 0; i < 400; i++) {
    store(holder, { id: i, payload: [i, i + 1] });
    churn();
    if (i % 50 === 0) $262.gc();
    assert.sameValue(holder.slot.id, i, "overwrite " + i);
    assert.sameValue(holder.slot.payload[1], i + 1, "payload " + i);
  }
  var fresh = {};
  $262.gc();
  $262.gc();
  for (var j = 0; j < 400; j++) {
    var target = { j: j };
    storeNew(target, { id: j });
    churn();
    keep.push(target);
    if (j % 80 === 0) $262.gc();
  }
  for (var m = 0; m < keep.length; m++) assert.sameValue(keep[m].late.id, m, "added " + m);
  var oldHolder = { v: null };
  $262.gc();
  $262.gc();
  function addLate(h, v) { h.added = v; }
  addLate(oldHolder, { id: "young" });
  churn();
  $262.gc();
  assert.sameValue(oldHolder.added.id, "young", "property added to an old object holds a young object");
})();
