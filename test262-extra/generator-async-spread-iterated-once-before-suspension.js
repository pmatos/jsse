/*---
description: >
  A spread element to the left of a `yield` or `await` in an array literal or
  an argument list is iterated exactly once: the values it produced are not
  iterated a second time after the suspension, so a patched
  `Array.prototype[Symbol.iterator]` observes one iteration per spread. Checked
  in generators the engine hosts as a bytecode chunk, in generators it runs as
  a state machine, async functions and async generators.
esid: sec-runtime-semantics-arrayaccumulation
info: |
  ArrayAccumulation
  SpreadElement : ... AssignmentExpression

  1. Let spreadRef be ? Evaluation of AssignmentExpression.
  2. Let spreadObj be ? GetValue(spreadRef).
  3. Let iteratorRecord be ? GetIterator(spreadObj, sync).
  4. Repeat, ... IteratorStepValue ... CreateDataPropertyOrThrow

  ArgumentListEvaluation : ... AssignmentExpression performs the same single
  GetIterator, and each element is evaluated before the next is.
flags: [async]
features: [generators, async-functions, async-iteration]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

var log = [];
function note(x) { log.push(x); return x; }

var originalIterator = Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = function() {
  note("arrayiter");
  return originalIterator.call(this);
};

var ga = [1];
var gb = [2];
function Pair(a, b) { this.v = a + "|" + b; }
function spreadJoin() { return Array.prototype.join.call(arguments); }

// `@` stands for `yield ` or `await `.
var cases = [
  ["arraySpreadThenSuspension", "return [...ga, @note(3)].join();", "arrayiter,3|1,3"],
  ["arrayTwoSpreads", "return [...ga, ...gb, @note(3)].join();", "arrayiter,arrayiter,3|1,2,3"],
  ["arraySpreadAmongElements", "return [note(0), ...ga, note(2), @note(3), ...gb].join();",
    "0,arrayiter,2,3,arrayiter|0,1,2,3,2"],
  ["arraySuspendingSpreadOperand", "return [...(@ga), note(2)].join();", "arrayiter,2|1,2"],
  ["callSpread", "return spreadJoin(...ga, @note(3));", "arrayiter,3|1,3"],
  ["callTwoSpreads", "return spreadJoin(...ga, ...gb, @note(3));", "arrayiter,arrayiter,3|1,2,3"],
  ["newSpread", "return new Pair(...ga, @note(3)).v;", "arrayiter,3|1|3"],
  ["methodCallSpread", "return { m: spreadJoin }.m(...ga, @note(3));", "arrayiter,3|1,3"],
  ["nestedSpread", "return [...[...ga, @note(3)], note(4)].join();", "arrayiter,3,arrayiter,4|1,3,4"],
  ["holesAndUndefinedSurvive", "var a = [undefined, , 3]; return JSON.stringify([...a, @note(4)]);",
    "arrayiter,4|[null,null,3,4]"],
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

jobs.reduce(function(p, job) { return p.then(job); }, Promise.resolve()).then(function() {
  Array.prototype[Symbol.iterator] = originalIterator;
}, function(e) {
  Array.prototype[Symbol.iterator] = originalIterator;
  throw e;
}).then($DONE, $DONE);
