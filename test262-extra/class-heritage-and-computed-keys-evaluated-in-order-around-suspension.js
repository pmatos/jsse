/*---
description: >
  In a generator, async function or async generator, a class whose heritage or
  computed element key contains a `yield`/`await` still evaluates its heritage
  and every computed key (ToPropertyKey included) in source order, before any
  static field initializer. Checked in generators the engine hosts as a
  bytecode chunk, in generators it runs as a state machine, async functions and
  async generators.
esid: sec-runtime-semantics-classdefinitionevaluation
info: |
  ClassDefinitionEvaluation

  ... 8. If ClassHeritage is present, then
    ... c. Let superclassRef be Completion(Evaluation of ClassHeritage).
    ... e. Let superclass be ? GetValue(superclassRef).
  ... 28. For each ClassElement e of elements, do
    a. If IsStatic of e is false, then
      i. Let element be Completion(ClassElementEvaluation of e with argument proto).
    b. Else, let element be Completion(ClassElementEvaluation of e with argument F).
  ... 31. For each element fieldRecord of staticElements, ... DefineField

  ClassElementName : ComputedPropertyName evaluates ToPropertyKey of the key.
flags: [async]
features: [generators, async-functions, async-iteration, class, class-fields-public, class-static-fields-public]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

// `@` stands for `yield ` or `await `.
var prelude = "var log = []; function n(x) { log.push(x); return x; }" +
  "function str(tag) { return { toString() { log.push(tag); return tag; } }; }";

var cases = [
  ["expressionHeritageKeysAndStaticField",
   "var C = class extends (n('heritage'), Object) { [n('k1')]() {} static [n('k2')] = n('sf'); [@n('k3')]() {} };" +
   "return log.join();",
   "heritage,k1,k2,k3,sf"],
  ["declarationHeritageKeysAndStaticField",
   "class C extends (n('heritage'), Object) { [n('k1')]() {} static [n('k2')] = n('sf'); [@n('k3')]() {} }" +
   "return log.join();",
   "heritage,k1,k2,k3,sf"],
  ["suspendingHeritage",
   "class C extends (n('h1'), @Object) { [n('k1')]() {} static [n('k2')] = n('sf'); }" +
   "return log.join();",
   "h1,k1,k2,sf"],
  ["keysAfterLastSuspensionStayInOrder",
   "class C { [n('k1')]() {} [@n('k2')]() {} [n('k3')]() {} static [n('k4')] = n('sf'); }" +
   "return log.join();",
   "k1,k2,k3,k4,sf"],
  ["toPropertyKeyOfEarlierKeyBeforeSuspension",
   "class C { [str('ts1')]() {} [@n('k2')]() {} }" +
   "return log.join();",
   "ts1,k2"],
  ["toPropertyKeyOfSuspendingKeyBeforeNextKey",
   "class C { [@str('ts1')]() {} [n('k2')]() {} }" +
   "return log.join();",
   "ts1,k2"],
  ["heritageBindingReadBeforeLaterKeyMutatesIt",
   "function A() {} function B() {} var Base = A;" +
   "class C extends Base { [(Base = B, 'x')]() {} [@'y']() {} }" +
   "return String(Object.getPrototypeOf(C) === A);",
   "true"],
  ["operandsAroundClassExpression",
   "var r = [n('pre'), class extends (n('h'), Object) { [n('k1')]() {} [@n('k2')]() {} }, n('post')];" +
   "return log.join();",
   "pre,h,k1,k2,post"],
  ["accessorAndFieldKeys",
   "class C { static get [n('g')]() { return 1; } [n('f')] = n('init'); static [n('s')] = n('sinit'); [@n('last')]() {} }" +
   "new C(); return log.join();",
   "g,f,s,last,sinit,init"],
  ["symbolAndNumericKeysKeepTheirIdentity",
   "var sym = Symbol('s');" +
   "class C { [sym]() { return 'sym'; } [1 + 1]() { return 'two'; } [@'z']() { return 'z'; } }" +
   "var c = new C(); return [c[sym](), c[2](), c.z(), Object.getOwnPropertyNames(C.prototype).join()].join();",
   "sym,two,z,2,constructor,z"],
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
