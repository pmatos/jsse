// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-canbeheldweakly
description: >
  CanBeHeldWeakly rejects exactly the Symbols that Symbol.for put in the
  GlobalSymbolRegistry; Symbol() symbols, well-known symbols and symbols with
  the same description as a registered one are accepted by WeakMap, WeakSet,
  WeakRef and FinalizationRegistry.
info: |
  CanBeHeldWeakly ( v )
    1. If v is an Object, return true.
    2. If v is a Symbol and KeyForSymbol(v) is undefined, return true.
    3. Return false.

  KeyForSymbol ( sym )
    1. For each element e of the GlobalSymbolRegistry List, do
      a. If SameValue(e.[[Symbol]], sym) is true, return e.[[Key]].
    2. Assert: GlobalSymbolRegistry does not currently contain an entry for sym.
    3. Return undefined.
features: [Symbol, WeakMap, WeakSet, WeakRef, FinalizationRegistry, symbols-as-weakmap-keys]
---*/

var registered = Symbol.for("canBeHeldWeakly.registered");
var plain = Symbol("canBeHeldWeakly.registered");
var sameDescription = Symbol("canBeHeldWeakly.registered");
var empty = Symbol();
var registeredEmpty = Symbol.for("");
var registeredUndefined = Symbol.for();
var registeredAgain = Symbol.for("canBeHeldWeakly.registered");

assert.sameValue(registered, registeredAgain, "Symbol.for returns the registry entry");
assert.sameValue(Symbol.keyFor(registered), "canBeHeldWeakly.registered");

var rejected = [registered, registeredEmpty, registeredUndefined, registeredAgain];
var accepted = [plain, sameDescription, empty, Symbol.iterator, Symbol.asyncIterator, Symbol.toStringTag];

rejected.forEach(function (sym) {
  var label = String(sym.description);
  var wm = new WeakMap();
  assert.throws(TypeError, function () { wm.set(sym, 1); }, "WeakMap.set registered " + label);
  assert.sameValue(wm.has(sym), false, "WeakMap.has registered " + label);
  assert.sameValue(wm.get(sym), undefined, "WeakMap.get registered " + label);
  assert.sameValue(wm.delete(sym), false, "WeakMap.delete registered " + label);

  var ws = new WeakSet();
  assert.throws(TypeError, function () { ws.add(sym); }, "WeakSet.add registered " + label);
  assert.sameValue(ws.has(sym), false, "WeakSet.has registered " + label);
  assert.sameValue(ws.delete(sym), false, "WeakSet.delete registered " + label);

  assert.throws(TypeError, function () { new WeakRef(sym); }, "WeakRef registered " + label);

  var fr = new FinalizationRegistry(function () {});
  assert.throws(TypeError, function () { fr.register(sym, 1); }, "register target " + label);
  assert.throws(TypeError, function () { fr.register({}, 1, sym); }, "register token " + label);
  assert.throws(TypeError, function () { fr.unregister(sym); }, "unregister token " + label);
});

accepted.forEach(function (sym) {
  var label = String(sym.description);
  var wm = new WeakMap();
  assert.sameValue(wm.set(sym, 7), wm, "WeakMap.set " + label);
  assert.sameValue(wm.has(sym), true, "WeakMap.has " + label);
  assert.sameValue(wm.get(sym), 7, "WeakMap.get " + label);
  assert.sameValue(wm.delete(sym), true, "WeakMap.delete " + label);
  assert.sameValue(wm.has(sym), false, "WeakMap.has after delete " + label);

  var ws = new WeakSet();
  assert.sameValue(ws.add(sym), ws, "WeakSet.add " + label);
  assert.sameValue(ws.has(sym), true, "WeakSet.has " + label);
  assert.sameValue(ws.delete(sym), true, "WeakSet.delete " + label);

  assert.sameValue(new WeakRef(sym).deref(), sym, "WeakRef " + label);

  var fr = new FinalizationRegistry(function () {});
  assert.sameValue(fr.register(sym, 1), undefined, "register target " + label);
  assert.sameValue(fr.register({}, 1, sym), undefined, "register token " + label);
  assert.sameValue(fr.unregister(sym), true, "unregister token " + label);
});

var viaConstructor = new WeakMap([[plain, 1], [Symbol.iterator, 2]]);
assert.sameValue(viaConstructor.get(plain), 1, "constructor iterable accepts plain symbols");
assert.throws(TypeError, function () {
  new WeakMap([[registered, 1]]);
}, "constructor iterable rejects a registered symbol");
assert.throws(TypeError, function () {
  new WeakSet([registered]);
}, "WeakSet constructor iterable rejects a registered symbol");

var ws = new WeakSet([plain, sameDescription]);
assert.sameValue(ws.has(plain), true);
assert.sameValue(ws.has(sameDescription), true);
assert.sameValue(ws.has(registered), false, "same description is not the registered symbol");

var wm = new WeakMap();
wm.set(plain, "plain");
wm.set(sameDescription, "other");
assert.sameValue(wm.get(plain), "plain", "keys with equal descriptions stay distinct");
assert.sameValue(wm.get(sameDescription), "other");
assert.sameValue(wm.get(registered), undefined);

assert.sameValue(
  Symbol.keyFor(registered), "canBeHeldWeakly.registered",
  "a rejected registered symbol stays registered"
);
