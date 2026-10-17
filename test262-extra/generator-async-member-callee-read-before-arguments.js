/*---
description: >
  A member or optional-chain callee is read, and `this` captured, before the
  arguments are evaluated, so a `yield` or `await` among the arguments cannot
  delay the property read: getters and Proxy traps run first, and a property
  reassigned during the suspension does not change the function called. Checked
  in generators the engine hosts as a bytecode chunk, in generators it runs as
  a state machine, async functions and async generators.
esid: sec-evaluatecall
info: |
  EvaluateCall ( func, ref, arguments, tailPosition )

  1. If ref is a Reference Record, then
    a. If IsPropertyReference(ref) is true, then
      i. Let thisValue be GetThisValue(ref).
  ...
  3. Let argList be ? ArgumentListEvaluation of arguments.

  CallExpression : CoverCallExpressionAndAsyncArrowHead

  3. Let func be ? GetValue(ref).

  EvaluateTemplate / sec-tagged-templates-runtime-semantics-evaluation read the
  tag's function with GetValue before evaluating the substitutions.
flags: [async]
features: [generators, async-functions, async-iteration, optional-chaining]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

var log = [];
function note(x) { log.push(x); return x; }

var go;
function key(n) { return { toString() { note("key" + n); return "m"; } }; }
function getterObject(name) {
  var o = {};
  Object.defineProperty(o, name, {
    get() {
      note("get " + name);
      return function(a, b) { return this === o ? "this ok " + a + b : "this wrong"; };
    }
  });
  return o;
}

// `@` stands for `yield ` or `await `.
var cases = [
  ["getterRunsBeforeArguments",
    "go = getterObject('m'); return go.m(note(1), @note(2));",
    "get m,1,2|this ok 12"],
  ["computedKeyConvertedBeforeArguments",
    "go = getterObject('m'); return go[key(1)](note(2), @note(3));",
    "key1,get m,2,3|this ok 23"],
  ["suspendingComputedKey",
    "go = getterObject('m'); return go[@key(1)](note(2));",
    "key1,get m,2|this ok 2undefined"],
  ["reassignedAfterRead",
    "go = { m(a) { return 'old' + a; } }; return go.m(@(go.m = function(a) { return 'new' + a; }, 1));",
    "|old1"],
  ["reassignedBaseAfterRead",
    "var a = { m() { return 'a'; } }, b = { m() { return 'b'; } }; go = a; return go.m(@(go = b, 1));",
    "|a"],
  ["primitiveBase",
    "String.prototype.sm = function(a, b) { 'use strict'; return typeof this + a + b; };" +
    " return 'str'.sm(note(1), @note(2));",
    "1,2|string12"],
  ["optionalChain",
    "go = getterObject('m'); return go?.m(note(1), @note(2));",
    "get m,1,2|this ok 12"],
  ["optionalChainNullish",
    "go = null; return go?.m(note(1), @note(2)) === undefined;",
    "|true"],
  ["optionalCall",
    "go = getterObject('m'); return go.m?.(note(1), @note(2));",
    "get m,1,2|this ok 12"],
  ["optionalCallNullish",
    "go = { m: undefined }; return go.m?.(note(1), @note(2)) === undefined;",
    "|true"],
  ["optionalCallLongerChain",
    "go = { m(a) { return { n(b) { return this.v + a + b; }, v: 'v' }; } }; return go.m?.(note(1)).n(@note(2));",
    "1,2|v12"],
  ["taggedTemplate",
    "go = getterObject('t'); go.tag = true; return go.t`${note(1)}${@note(2)}`;",
    "get t,1,2|this ok ,,1"],
];

function variants(src) {
  var out = [];
  out.push(["generator", () => GeneratorFunction(src.replace(/@/g, "yield ")), "sync"]);
  out.push(["generator with yield*", () => GeneratorFunction("if (false) yield* [];" + src.replace(/@/g, "yield ")), "sync"]);
  out.push(["async function", () => AsyncFunction(src.replace(/@/g, "await ")), "async"]);
  out.push(["async generator", () => AsyncGeneratorFunction(src.replace(/@/g, "yield ")), "asyncgen"]);
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
      log = [];
      var fn = v[1]();
      var run = v[2] === "sync" ? runSync : v[2] === "async" ? function(f) { return f(); } : runAsyncGen;
      return run(fn).then(function(value) {
        assert.sameValue(log.join() + "|" + value, c[2], c[0] + " (" + v[0] + ")");
      });
    });
  });
});

class Base {
  m(a, b) { return "Base " + this.tag + a + b; }
  get gm() { note("get gm"); return function(a) { return "Base getter " + this.tag + a; }; }
}
class Derived extends Base {
  constructor() { super(); this.tag = "D"; }
  m() { throw new Test262Error("Derived.m must not be called through super"); }
  *genYieldStar() { yield* []; return super.m(note(1), yield note(2)); }
  *gen() { return super.m(note(1), yield note(2)); }
  async asyncMethod() { return super.m(note(1), await note(2)); }
  async *asyncGen() { return super.m(note(1), yield note(2)); }
  *getter() { yield* []; return super.gm(yield note(1)); }
  #p(a) { return "private " + this.tag + a; }
  *privateGen() { yield* []; return this.#p(yield note(1)); }
  async privateAsync() { return this.#p(await note(1)); }
  async asyncGetter() { return super.gm(await note(1)); }
}

[
  ["superGeneratorYieldStar", d => runSync(() => d.genYieldStar()), "1,2", "Base D12"],
  ["superGenerator", d => runSync(() => d.gen()), "1,2", "Base D12"],
  ["superAsync", d => d.asyncMethod(), "1,2", "Base D12"],
  ["superAsyncGenerator", d => runAsyncGen(() => d.asyncGen()), "1,2", "Base D12"],
  ["superGetterGenerator", d => runSync(() => d.getter()), "get gm,1", "Base getter D1"],
  ["superGetterAsync", d => d.asyncGetter(), "get gm,1", "Base getter D1"],
  ["privateGenerator", d => runSync(() => d.privateGen()), "1", "private D1"],
  ["privateAsync", d => d.privateAsync(), "1", "private D1"],
].forEach(function(c) {
  jobs.push(function() {
    log = [];
    return c[1](new Derived()).then(function(value) {
      assert.sameValue(log.join() + "|" + value, c[2] + "|" + c[3], c[0]);
    });
  });
});

jobs.push(function() {
  log = [];
  var target = { m() { return "target"; } };
  var proxy = new Proxy(target, {
    get(t, k, r) { note("get " + String(k)); return Reflect.get(t, k, r); }
  });
  var fn = AsyncFunction("p", "return p.m(note('arg'), await 1);");
  return fn(proxy).then(function(value) {
    assert.sameValue(log.join() + "|" + value, "get m,arg|target", "proxy get trap runs before the arguments");
  });
});

jobs.reduce(function(p, job) { return p.then(job); }, Promise.resolve()).then($DONE, $DONE);
