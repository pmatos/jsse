/*---
description: >
  In a generator, async function or async generator, a class with a suspending
  computed key performs the ClassHeritage checks (IsConstructor, the read and
  type check of the superclass's "prototype") right after the heritage is
  evaluated, before the first ClassElement key, and reads "prototype" exactly
  once.
esid: sec-runtime-semantics-classdefinitionevaluation
info: |
  ClassDefinitionEvaluation

  8. If ClassHeritage is present, then
    ... e. Let superclass be ? GetValue(superclassRef).
    f. If superclass is null, then
      i. Let protoParent be null.
      ii. Let constructorParent be %Function.prototype%.
    g. Else if IsConstructor(superclass) is false, then
      i. Throw a TypeError exception.
    h. Else,
      i. Let protoParent be ? Get(superclass, "prototype").
      ii. If protoParent is not an Object and protoParent is not null, throw a TypeError exception.
  ... 28. For each ClassElement e of elements, do ... ClassElementEvaluation
flags: [async]
features: [generators, async-functions, async-iteration, class]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

// `@` stands for `yield ` or `await `.
var prelude = "var log = []; function n(x) { log.push(x); return x; }" +
  "function trapped(read) {" +
  "  return new Proxy(function() {}, { get(t, k) { return k === 'prototype' ? read() : t[k]; } });" +
  "}" +
  "function proxied(proto) {" +
  "  var f = function() {}; if (proto !== undefined) f.prototype = proto;" +
  "  return new Proxy(f, { get(t, k) { if (k === 'prototype') log.push('protoget'); return t[k]; } });" +
  "}";

var cases = [
  ["nonConstructorHeritageThrowsBeforeFirstKey",
   "try { class C extends 5 { [n('k1')]() {} [@n('k2')]() {} } } catch (e) { log.push(e.constructor.name); }" +
   "return log.join();",
   "TypeError"],
  ["nonConstructorHeritageObjectThrowsBeforeFirstKey",
   "try { class C extends (() => {}) { [n('k1')]() {} [@n('k2')]() {} } } catch (e) { log.push(e.constructor.name); }" +
   "return log.join();",
   "TypeError"],
  ["prototypeReadBeforeFirstKeyAndOnce",
   "var P = proxied();" +
   "class D extends P { [n('k1')]() {} [@n('k2')]() {} [n('k3')]() {} }" +
   "return log.join();",
   "protoget,k1,k2,k3"],
  ["prototypeReadBeforeKeysInExpression",
   "var P = proxied();" +
   "var D = class extends P { [n('k1')]() {} [@n('k2')]() {} };" +
   "return log.join();",
   "protoget,k1,k2"],
  ["prototypeReadAfterSuspendingHeritage",
   "var P = proxied();" +
   "class D extends (n('h'), @P) { [n('k1')]() {} [@n('k2')]() {} }" +
   "return log.join();",
   "h,protoget,k1,k2"],
  ["invalidPrototypeThrowsBeforeFirstKey",
   "var P = proxied(3);" +
   "try { class D extends P { [n('k1')]() {} [@n('k2')]() {} } } catch (e) { log.push(e.constructor.name); }" +
   "return log.join();",
   "protoget,TypeError"],
  ["prototypeGetterThrowsBeforeFirstKey",
   "var G = trapped(() => { log.push('getter'); throw new RangeError('x'); });" +
   "try { class D extends G { [n('k1')]() {} [@n('k2')]() {} } } catch (e) { log.push(e.constructor.name); }" +
   "return log.join();",
   "getter,RangeError"],
  ["prototypeChangedByKeyDoesNotAffectClass",
   "function B() {} var orig = B.prototype;" +
   "class C extends B { [(B.prototype = {}, 'x')]() {} [@'y']() {} }" +
   "return String(Object.getPrototypeOf(C.prototype) === orig);",
   "true"],
  ["prototypeGetterRunsOnceWithKeyMutation",
   "var count = 0; var orig = {}; var B = trapped(() => { count++; return orig; });" +
   "class C extends B { [n('k1')]() {} [@n('k2')]() {} }" +
   "return [count, Object.getPrototypeOf(C.prototype) === orig].join();",
   "1,true"],
  ["nullHeritage",
   "class C extends null { [n('k1')]() {} [@n('k2')]() {} }" +
   "return [String(Object.getPrototypeOf(C.prototype)), Object.getPrototypeOf(C) === Function.prototype, log.join()].join('|');",
   "null|true|k1,k2"],
  ["nullHeritageFromSuspension",
   "class C extends (@null) { [n('k1')]() {} [@n('k2')]() {} }" +
   "return [String(Object.getPrototypeOf(C.prototype)), Object.getPrototypeOf(C) === Function.prototype, log.join()].join('|');",
   "null|true|k1,k2"],
  ["prototypeNullAllowed",
   "function B() {} B.prototype = null;" +
   "class C extends B { [n('k1')]() {} [@n('k2')]() {} }" +
   "return [String(Object.getPrototypeOf(C.prototype)), Object.getPrototypeOf(C) === B, log.join()].join('|');",
   "null|true|k1,k2"],
  ["prototypeNullWithoutSuspension",
   "function B() {} B.prototype = null;" +
   "class C extends B { [n('k1')]() {} }" +
   "return [String(Object.getPrototypeOf(C.prototype)), Object.getPrototypeOf(C) === B, log.join()].join('|');",
   "null|true|k1"],
  ["regularInheritance",
   "class A { m() { return 'A.m'; } static s() { return 'A.s'; } }" +
   "class C extends A { [n('k1')]() {} [@'k2']() {} }" +
   "return [new C().m(), C.s(), Object.getPrototypeOf(C) === A, Object.getPrototypeOf(C.prototype) === A.prototype].join();",
   "A.m,A.s,true,true"],
  ["noSuspendingKeyKeepsOrder",
   "var P = proxied();" +
   "class D extends P { [n('k1')]() {} [n('k2')]() {} }" +
   "return log.join();",
   "protoget,k1,k2"],
];

function variants(src) {
  var body = prelude + src;
  var out = [];
  out.push(["generator", () => GeneratorFunction(body.replace(/@/g, "yield ")), "sync"]);
  out.push(["generator with yield*", () => GeneratorFunction("if (false) yield* [];" + body.replace(/@/g, "yield ")), "sync"]);
  out.push(["async function", () => AsyncFunction(body.replace(/@/g, "await ")), "async"]);
  out.push(["async generator", () => AsyncGeneratorFunction(body.replace(/@/g, "yield ")), "asyncgen"]);
  return out;
}

function runSync(fn) {
  var it = fn();
  var r = it.next();
  while (!r.done) r = it.next(r.value);
  return Promise.resolve(r.value);
}

function runAsyncGen(fn) {
  var it = fn();
  function step(r) {
    return r.done ? r.value : it.next(r.value).then(step);
  }
  return it.next().then(step);
}

var jobs = [];
cases.forEach(function(c) {
  variants(c[1]).forEach(function(v) {
    jobs.push(function() {
      var fn = v[1]();
      var run = v[2] === "sync" ? runSync : v[2] === "async" ? function(f) { return f(); } : runAsyncGen;
      return run(fn).then(function(value) {
        assert.sameValue(value, c[2], c[0] + " (" + v[0] + ")");
      });
    });
  });
});

jobs.reduce(function(p, job) { return p.then(job); }, Promise.resolve()).then($DONE, $DONE);
