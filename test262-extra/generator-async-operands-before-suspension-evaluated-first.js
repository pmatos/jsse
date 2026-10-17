/*---
description: >
  Operands to the left of a `yield` or `await` are evaluated, and their values
  read, before the suspension: binary operands, callee and arguments, `new`,
  array and object literals (computed keys, spread, shorthand), and template
  substitutions. Checked in generators the engine hosts as a bytecode chunk, in
  generators it runs as a state machine, async functions and async generators.
esid: sec-evaluatecall
info: |
  EvaluateCall: ArgumentListEvaluation evaluates the arguments left to right,
  and a `yield`/`await` among them suspends in the middle, so every argument to
  its left has already been evaluated. The same holds for
  sec-runtime-semantics-arrayaccumulation, sec-object-initializer-runtime-semantics-evaluation,
  sec-applystringornumericbinaryoperator and sec-template-literals-runtime-semantics-evaluation.
flags: [async]
includes: [compareArray.js]
features: [generators, async-functions, async-iteration]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

var log = [];
function note(x) { log.push(x); return x; }

var gx, gf, go, gs, gsrc;
function Pair(a, b) { this.v = a + "|" + b; }
function join3(a, b, c) { return [a, b, c].join("+"); }
function spreadJoin() { return Array.prototype.join.call(arguments); }
var gi = {
  [Symbol.iterator]() {
    note("iter");
    var n = 0;
    return { next() { note("next" + n); return { done: n++ >= 1, value: n }; } };
  }
};
function key(n) { return { toString() { note("key" + n); return "k" + n; } }; }
var to = { toString() { note("ts"); return "S"; } };
var gt = { tag: "G", t(s, ...v) { return this.tag + v.join(); } };

// `@` stands for `yield ` or `await `.
var cases = [
  ["binary", "return note(1) + (@note(2)) * note(3);", "1,2,3|7"],
  ["binaryReadsOperandFirst", "gx = 1; return gx + (@(gx = 10));", "|11"],
  ["callArguments", "return join3(note(1), @note(2), note(3));", "1,2,3|1+2+3"],
  ["calleeIsReadFirst",
    "gf = (a, b) => 'old' + a + b; return gf(note(1), @(gf = (a, b) => 'new', note(2)));",
    "1,2|old12"],
  ["methodCallKeepsThis",
    "go = { tag: 'T', m(a, b) { return this.tag + a + b; } }; return go.m(note(1), @note(2));",
    "1,2|T12"],
  ["withScopeCallee",
    "with ({ wf(a, b) { return this.t + a + b; }, t: 'W' }) { return wf(note(1), @note(2)); }",
    "1,2|W12"],
  ["directEvalCallee", "var loc = 5; return eval('loc', @note(2));", "2|5"],
  ["newExpression", "return new Pair(note(1), @note(2)).v;", "1,2|1|2"],
  ["callSpreadIteratesFirst", "return spreadJoin(...gi, @note(2));", "iter,next0,next1,2|1,2"],
  ["arrayElements", "return [note(1), @note(2), note(3), @note(4)].join();", "1,2,3,4|1,2,3,4"],
  ["arraySpreadIteratesFirst", "return [...gi, @note(2), note(3)].join();", "iter,next0,next1,2,3|1,2,3"],
  ["objectComputedKeys",
    "var o = { [key(1)]: note('v1'), [key(2)]: @note('v2'), [key(3)]: note('v3') };" +
    " return Object.keys(o).join() + JSON.stringify(o);",
    "key1,v1,key2,v2,key3,v3|k1,k2,k3{\"k1\":\"v1\",\"k2\":\"v2\",\"k3\":\"v3\"}"],
  ["objectShorthandReadFirst", "gs = 'a'; var o = { gs, x: @(gs = 'b') }; return o.gs + o.x;", "|ab"],
  ["objectSpreadCopiesFirst",
    "gsrc = { get p() { note('get p'); return 1; } }; var o = { ...gsrc, q: @note(2) };" +
    " return JSON.stringify(o);",
    "get p,2|{\"p\":1,\"q\":2}"],
  ["templateConvertsFirst", "return `${to}-${@note(2)}-${to}`;", "ts,2,ts|S-2-S"],
  ["taggedTemplateKeepsThis", "return gt.t`${note(1)}${@note(2)}`;", "1,2|G1,2"],
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

jobs.reduce(function(p, job) { return p.then(job); }, Promise.resolve()).then($DONE, $DONE);
