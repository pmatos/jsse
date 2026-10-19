// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-runtime-semantics-keyedbindinginitialization
description: >
  A `let`/`const` object binding pattern made of plain names (`{a, b: c,
  "d": e, 1: f}`) reads each property once, in source order, with GetV, after
  RequireObjectCoercible, and binds it before reading the next. The pattern
  runs in function bodies, blocks and switch cases alike.
info: |
  BindingInitialization : ObjectBindingPattern

  1. Perform ? RequireObjectCoercible(value).
  2. Perform ? PropertyBindingInitialization of BindingPropertyList with
     arguments value and environment.

  KeyedBindingInitialization : SingleNameBinding

  1. Let bindingId be the StringValue of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(bindingId, environment).
  3. Let v be ? GetV(value, propertyName).
  ...
  6. Return ? InitializeReferencedBinding(lhs, v).
flags: [noStrict]
features: [Proxy, Symbol, BigInt, destructuring-binding]
includes: [compareArray.js]
---*/

function shapes() {
  var o = { a: 1, b: 2, "c d": 3, 1: 4, length: 5 };
  const { a, b: bb, "c d": cd, 1: one, length } = o;
  return [a, bb, cd, one, length];
}
assert.compareArray(shapes(), [1, 2, 3, 4, 5]);

function numericKeyForms() {
  const { 0: zero, 1.5: frac, 1e3: big } = { 0: "z", "1.5": "f", 1000: "b" };
  return [zero, frac, big];
}
assert.compareArray(numericKeyForms(), ["z", "f", "b"]);

function missingAndInherited() {
  const proto = { inherited: "p" };
  const o = Object.create(proto);
  o.own = "o";
  const { own, inherited, absent } = o;
  return [own, inherited, absent];
}
assert.compareArray(missingAndInherited(), ["o", "p", undefined]);

function arrayAndStringSources() {
  const { 0: a0, length: alen } = ["x", "y"];
  const { 1: s1, length: slen, toString: ts } = "hey";
  return [a0, alen, s1, slen, ts === String.prototype.toString];
}
assert.compareArray(arrayAndStringSources(), ["x", 2, "e", 3, true]);

var log = [];
function trace(name, value) {
  return { get [name]() { log.push("get " + name); return value; } };
}

function getterOrder() {
  var src = {
    get a() { log.push("get a"); return 1; },
    get b() { log.push("get b"); return 2; },
    get c() { log.push("get c"); return 3; },
  };
  const { c, a, b: renamed } = src;
  log.push("bound " + [c, a, renamed].join());
  return log.slice();
}
assert.compareArray(
  getterOrder(),
  ["get c", "get a", "get b", "bound 3,1,2"],
  "properties are read in pattern order, once each"
);

function bindingBeforeNextRead() {
  var seen;
  var src = {
    get first() { return "f"; },
    get second() {
      seen = "second read";
      return "s";
    },
  };
  const { first, second } = src;
  return [first, second, seen];
}
assert.compareArray(bindingBeforeNextRead(), ["f", "s", "second read"]);

function getterThrowsMidPattern() {
  var reads = [];
  var src = {
    get a() { reads.push("a"); return 1; },
    get b() { reads.push("b"); throw new RangeError("b"); },
    get c() { reads.push("c"); return 3; },
  };
  try {
    const { a, b, c } = src;
    return "no throw";
  } catch (e) {
    return e.constructor.name + ":" + reads.join();
  }
}
assert.sameValue(getterThrowsMidPattern(), "RangeError:a,b");

function proxyTrapOrder() {
  var traps = [];
  var target = { x: 1, y: 2 };
  var proxy = new Proxy(target, {
    get(t, key, receiver) {
      traps.push("get " + String(key) + " " + (receiver === proxy));
      return Reflect.get(t, key, receiver);
    },
    getOwnPropertyDescriptor(t, key) { traps.push("gopd " + String(key)); return Reflect.getOwnPropertyDescriptor(t, key); },
    has(t, key) { traps.push("has " + String(key)); return Reflect.has(t, key); },
    ownKeys(t) { traps.push("ownKeys"); return Reflect.ownKeys(t); },
  });
  const { y, x } = proxy;
  return [y, x, traps.join("|")];
}
var proxied = proxyTrapOrder();
assert.sameValue(proxied[0], 2);
assert.sameValue(proxied[1], 1);
assert.sameValue(proxied[2], "get y true|get x true", "only [[Get]] traps, with the proxy as receiver");

function symbolKeyIsNotTouched() {
  var sym = Symbol("s");
  var src = { [sym]: "sym", s: "str" };
  const { s } = src;
  return s;
}
assert.sameValue(symbolKeyIsNotTouched(), "str");

function primitiveReceivers() {
  var seen = [];
  function getter() { "use strict"; seen.push(typeof this); return typeof this; }
  Object.defineProperty(Number.prototype, "probe", { get: getter, configurable: true });
  Object.defineProperty(Boolean.prototype, "probe", { get: getter, configurable: true });
  Object.defineProperty(Symbol.prototype, "probe", { get: getter, configurable: true });
  Object.defineProperty(BigInt.prototype, "probe", { get: getter, configurable: true });
  try {
    const { probe: n } = 1;
    const { probe: b } = true;
    const { probe: s } = Symbol();
    const { probe: i } = 1n;
    return [n, b, s, i, seen.join()];
  } finally {
    delete Number.prototype.probe;
    delete Boolean.prototype.probe;
    delete Symbol.prototype.probe;
    delete BigInt.prototype.probe;
  }
}
assert.compareArray(
  primitiveReceivers(),
  ["number", "boolean", "symbol", "bigint", "number,boolean,symbol,bigint"]
);

function nullishThrowsBeforeAnyRead() {
  var results = [];
  var thrown = [];
  [null, undefined].forEach(function (value) {
    try {
      const { a, b } = value;
      results.push("no throw");
    } catch (e) {
      thrown.push(e.constructor === TypeError);
    }
  });
  return thrown.join();
}
assert.sameValue(nullishThrowsBeforeAnyRead(), "true,true");

function emptyPatternStillChecks() {
  var out = [];
  for (var value of [null, undefined, 0, "", false, {}]) {
    try {
      const {} = value;
      out.push("ok");
    } catch (e) {
      out.push(e.constructor.name);
    }
  }
  return out.join();
}
assert.sameValue(emptyPatternStillChecks(), "TypeError,TypeError,ok,ok,ok,ok");

function emptyPatternReadsNothing() {
  var reads = 0;
  const {} = new Proxy({}, { get() { reads++; }, ownKeys() { reads++; return []; } });
  return reads;
}
assert.sameValue(emptyPatternReadsNothing(), 0);

function tdzInInitializer() {
  try {
    const { a, b } = { a: b, b: 1 };
    return "no throw";
  } catch (e) {
    return e.constructor.name;
  }
}
assert.sameValue(tdzInInitializer(), "ReferenceError");

function tdzSelfReference() {
  try {
    let { a } = a;
    return "no throw";
  } catch (e) {
    return e.constructor.name;
  }
}
assert.sameValue(tdzSelfReference(), "ReferenceError");

function tdzInsideGetterBeforeBinding() {
  var observed;
  var src = {
    get first() {
      try { second; observed = "no throw"; } catch (e) { observed = e.constructor.name; }
      return 1;
    },
    second: 2,
  };
  const { first, second } = src;
  return [observed, first, second];
}
assert.compareArray(tdzInsideGetterBeforeBinding(), ["ReferenceError", 1, 2],
  "a later name is still in TDZ while an earlier getter runs");

function getterSeesEarlierBinding() {
  var observed;
  var src = {
    first: 1,
    get second() {
      try { first; observed = "bound"; } catch (e) { observed = "tdz"; }
      return 2;
    },
  };
  const { first, second } = src;
  return observed;
}
assert.sameValue(getterSeesEarlierBinding(), "bound",
  "a name bound by an earlier property is initialised before the next read");

function constIsConst() {
  const { a } = { a: 1 };
  try { a = 2; return "assigned"; } catch (e) { return e.constructor.name + a; }
}
assert.sameValue(constIsConst(), "TypeError1");

function letIsMutable() {
  let { a, b } = { a: 1, b: 2 };
  a += 10; b = "x";
  return a + b;
}
assert.sameValue(letIsMutable(), "11x");

function blockScopes() {
  var out = [];
  const { v } = { v: "outer" };
  {
    const { v, w } = { v: "inner", w: "w" };
    out.push(v, w);
  }
  switch (1) {
    case 1:
      let { v: sv } = { v: "case" };
      out.push(sv);
  }
  out.push(v);
  return out.join();
}
assert.sameValue(blockScopes(), "inner,w,case,outer");

function closuresCaptureBindings() {
  const { a, b } = { a: 1, b: 2 };
  var f = function () { return a + b; };
  return f();
}
assert.sameValue(closuresCaptureBindings(), 3);

function loopBodyFreshBindings() {
  var fns = [];
  for (var i = 0; i < 3; i++) {
    const { n } = { n: i };
    fns.push(function () { return n; });
  }
  return fns.map(function (f) { return f(); }).join();
}
assert.sameValue(loopBodyFreshBindings(), "0,1,2");

function multipleDeclarators() {
  const { a } = { a: 1 }, { b } = { b: 2 }, c = a + b;
  return [a, b, c];
}
assert.compareArray(multipleDeclarators(), [1, 2, 3]);

function shadowsParameterAndGlobal() {
  const { log } = { log: "shadowed" };
  return log;
}
assert.sameValue(shadowsParameterAndGlobal(), "shadowed");

function sameSourceTwice() {
  var count = 0;
  var src = { get a() { count++; return count; } };
  const { a: first } = src;
  const { a: second } = src;
  return [first, second, count];
}
assert.compareArray(sameSourceTwice(), [1, 2, 2]);

function sourceMutatedByGetter() {
  var src = {
    get a() { delete this.b; return 1; },
    b: 2,
  };
  const { a, b } = src;
  return [a, b];
}
assert.compareArray(sourceMutatedByGetter(), [1, undefined]);

function resultOfPatternDeclarationInLoopAndTry() {
  var out = [];
  for (var item of [{ k: 1 }, { k: 2 }]) {
    try {
      const { k } = item;
      if (k === 2) throw new Error("two");
      out.push(k);
    } catch (e) {
      out.push(e.message);
    } finally {
      const { k: fk } = item;
      out.push("f" + fk);
    }
  }
  return out.join();
}
assert.sameValue(resultOfPatternDeclarationInLoopAndTry(), "1,f1,two,f2");

function arrowAndMethodBodies() {
  var arrow = (o) => { const { p, q } = o; return p + q; };
  var obj = { m(o) { let { p, q } = o; return p * q; } };
  return [arrow({ p: 2, q: 3 }), obj.m({ p: 2, q: 3 })];
}
assert.compareArray(arrowAndMethodBodies(), [5, 6]);

function nestedFunctionUsesOwnPattern() {
  const { a } = { a: 1 };
  function inner() { const { a } = { a: 2 }; return a; }
  return [a, inner()];
}
assert.compareArray(nestedFunctionUsesOwnPattern(), [1, 2]);

function throwInInitializerLeavesNothingBound() {
  try {
    const { a } = (function () { throw new SyntaxError("init"); })();
  } catch (e) {
    return e.constructor.name;
  }
}
assert.sameValue(throwInInitializerLeavesNothingBound(), "SyntaxError");

function patternAsLastStatementValue() {
  const { a, b } = { a: 1, b: 2 };
  return a + b;
}
assert.sameValue(patternAsLastStatementValue(), 3);

function typedArrayAndArgumentsSources() {
  const { 0: t0, length: tl } = new Uint8Array([9, 8]);
  const { 0: a0, length: al } = (function () { return arguments; })("x", "y");
  return [t0, tl, a0, al];
}
assert.compareArray(typedArrayAndArgumentsSources(), [9, 2, "x", 2]);

function proxyOverArrayIndexKey() {
  var keys = [];
  var proxy = new Proxy([5, 6], { get(t, k, r) { keys.push(String(k)); return Reflect.get(t, k, r); } });
  const { 1: second, 0: first } = proxy;
  return [first, second, keys.join()];
}
assert.compareArray(proxyOverArrayIndexKey(), [5, 6, "1,0"]);

function lateBoundPrototypeGetter() {
  var holder = Object.create({ get inherited() { return this === holder ? "receiver ok" : "wrong receiver"; } });
  const { inherited } = holder;
  return inherited;
}
assert.sameValue(lateBoundPrototypeGetter(), "receiver ok");

function protoKeyIsAnAccessorOnObjectPrototype() {
  const { __proto__: p, x } = { x: 1 };
  const { __proto__ } = Object.create(null, { y: { value: 2 } });
  var own = {};
  Object.defineProperty(own, "__proto__", { value: 7, enumerable: true });
  const { __proto__: q } = own;
  return [p === Object.prototype, x, __proto__, q];
}
assert.compareArray(protoKeyIsAnAccessorOnObjectPrototype(), [true, 1, undefined, 7]);

function* yieldInInitializer() {
  const { a, b } = yield "first";
  const { c } = { c: yield "second" };
  return a + b + c;
}
var gen = yieldInInitializer();
assert.sameValue(gen.next().value, "first");
assert.sameValue(gen.next({ a: 1, b: 2 }).value, "second");
assert.sameValue(gen.next(3).value, 6);

function keywordKeysAndWrapperSources() {
  const { default: d, if: i, class: c } = { default: 1, if: 2, class: 3 };
  const { constructor: C, name } = function named() {};
  return [d + i + c, C === Function, name];
}
assert.compareArray(keywordKeysAndWrapperSources(), [6, true, "named"]);
