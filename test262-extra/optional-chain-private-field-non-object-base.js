// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-privateget
description: >
  A private-name read reached through an optional chain throws a TypeError when
  the base is not an object, or when the private accessor has no getter, just
  like the same read outside an optional chain.
info: |
  13.3.7.2 Runtime Semantics: ChainEvaluation
    OptionalChain : OptionalChain . PrivateIdentifier
      4. Return MakePrivateReference(newValue, fieldNameString).

  6.2.5.5 GetValue ( V )
    3.a. Let baseObj be ? ToObject(V.[[Base]]).
    3.b. If IsPrivateReference(V) is true, then
      i. Return ? PrivateGet(baseObj, V.[[ReferencedName]]).

  7.3.28 PrivateGet ( O, P )
    1. Let entry be PrivateElementFind(O, P).
    2. If entry is empty, throw a TypeError exception.
    5. If entry.[[Get]] is undefined, throw a TypeError exception.
features: [class, class-fields-private, class-methods-private, class-static-methods-private, optional-chaining]
---*/

class C {
  #x = 1;
  static tail(o) { return o?.value.#x; }
  static base(o) { return o.#x?.y; }
}

class D {
  #x = { y: "found" };
  static base(o) { return o.#x?.y; }
}

class E {
  #x = null;
  static base(o) { return o.#x?.y; }
}

class F {
  set #x(v) {}
  static base(o) { return o.#x?.y; }
}

class G {
  get #x() { return { y: "accessor" }; }
  static base(o) { return o.#x?.y; }
}

class H {
  #m() { return this; }
  static call(o) { return o.#m?.(); }
  static tail(o) { return o?.value.#m; }
  static base(o) { return o.#m?.name; }
}

// Non-optional private link inside a chain: `o?.value.#x`.
assert.throws(TypeError, function() { C.tail({ value: 5 }); }, "number base");
assert.throws(TypeError, function() { C.tail({ value: "str" }); }, "string base");
assert.throws(TypeError, function() { C.tail({ value: true }); }, "boolean base");
assert.throws(TypeError, function() { C.tail({ value: Symbol() }); }, "symbol base");
assert.throws(TypeError, function() { C.tail({ value: 1n }); }, "bigint base");
assert.throws(TypeError, function() { C.tail({ value: {} }); }, "object without the private name");
assert.throws(TypeError, function() { C.tail({ value: null }); }, "null base");
assert.throws(TypeError, function() { C.tail({ value: undefined }); }, "undefined base");
assert.sameValue(C.tail({ value: new C() }), 1, "instance with the private name");
assert.sameValue(C.tail(null), undefined, "short-circuit on null");
assert.sameValue(C.tail(undefined), undefined, "short-circuit on undefined");
assert.throws(TypeError, function() { H.tail({ value: 5 }); }, "private method, number base");
assert.sameValue(typeof H.tail({ value: new H() }), "function", "private method, instance");

// Private name as the base of an optional chain: `o.#x?.y`.
assert.throws(TypeError, function() { C.base(5); }, "number base");
assert.throws(TypeError, function() { C.base("str"); }, "string base");
assert.throws(TypeError, function() { C.base(true); }, "boolean base");
assert.throws(TypeError, function() { C.base(Symbol()); }, "symbol base");
assert.throws(TypeError, function() { C.base(1n); }, "bigint base");
assert.throws(TypeError, function() { C.base(null); }, "null base");
assert.throws(TypeError, function() { C.base(undefined); }, "undefined base");
assert.throws(TypeError, function() { C.base({}); }, "object without the private name");
assert.sameValue(C.base(new C()), undefined, "field holds a number without y");
assert.sameValue(D.base(new D()), "found", "field holds an object, chain continues");
assert.sameValue(E.base(new E()), undefined, "field holds null, chain short-circuits");
assert.sameValue(G.base(new G()), "accessor", "getter result, chain continues");
assert.throws(TypeError, function() { H.base(5); }, "private method, number base");
assert.sameValue(H.base(new H()), "#m", "private method, chain continues");

// A private-name base of `?.()` supplies the receiver as `this`.
var h = new H();
assert.sameValue(H.call(h), h, "private method called through ?.() gets the base as this");
assert.throws(TypeError, function() { H.call(5); }, "?.() on private method, number base");

// Set-only private accessor as the base of an optional chain.
assert.throws(TypeError, function() { F.base(new F()); }, "set-only accessor has no getter");
