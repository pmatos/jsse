/*---
description: A for/for-in/for-of loop that can capture its scope keeps a fresh per-iteration environment; one that cannot behaves identically
info: |
  14.7.4.3 ForBodyEvaluation / 14.7.4.4 CreatePerIterationEnvironment
  14.7.5.7 ForIn/OfBodyEvaluation step 5.j (NewDeclarativeEnvironment per iteration)

  The per-iteration environment of a loop whose head, test, update and body
  contain no function, class, `with` or direct `eval` is unobservable, so an
  implementation may reuse one environment for the whole loop. Every shape
  below that CAN observe the binding must still see a distinct binding per
  iteration (or, for the head initializer, the frozen initial binding), both in
  an ordinary function and in a generator, where each loop body also contains a
  `yield` so the loop is driven by the generator state machine.
esid: sec-forbodyevaluation
includes: [compareArray.js]
flags: [noStrict]
---*/

var GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;

function runPlain(source) {
  return new Function(source.split("/*Y*/").join(""))();
}

function runGenerator(source) {
  var it = new GeneratorFunction(source.split("/*Y*/").join("yield 0;"))();
  var step = it.next();
  while (!step.done) {
    step = it.next();
  }
  return step.value;
}

function check(name, source, expected) {
  assert.compareArray(runPlain(source), expected, name + " (function)");
  assert.compareArray(runGenerator(source), expected, name + " (generator)");
}

check(
  "closure in the for head initializer sees the frozen initial binding",
  "var fs = []; for (let i = 0, f = () => i; i < 3; i++) { /*Y*/ fs.push(f); } return fs.map(f => f());",
  [0, 0, 0]
);

check(
  "closure in the for test sees the binding of its own iteration",
  "var fs = []; for (let i = 0; fs.push(() => i), i < 3; i++) { /*Y*/ } return fs.map(f => f());",
  [0, 1, 2, 3]
);

check(
  "closure in the for update sees the next iteration's binding",
  "var fs = []; for (let i = 0; i < 3; fs.push(() => i), i++) { /*Y*/ } return fs.map(f => f());",
  [1, 2, 3]
);

check(
  "arrow in the for body",
  "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ fs.push(() => i); } return fs.map(f => f());",
  [0, 1, 2]
);

check(
  "function declaration in the for body",
  "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ function f() { return i; } fs.push(f); } return fs.map(f => f());",
  [0, 1, 2]
);

check(
  "class in the for body",
  "var cs = []; for (let i = 0; i < 3; i++) { /*Y*/ class C { static get v() { return i; } } cs.push(C); } return cs.map(C => C.v);",
  [0, 1, 2]
);

check(
  "direct eval in the for body",
  "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ fs.push(eval('() => i')); } return fs.map(f => f());",
  [0, 1, 2]
);

check(
  "closure in a destructuring default in the for body",
  "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ let { f = () => i } = {}; fs.push(f); } return fs.map(f => f());",
  [0, 1, 2]
);

check(
  "closure in a catch parameter default in the for body",
  "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ try { throw {}; } catch ({ f = () => i }) { fs.push(f); } } return fs.map(f => f());",
  [0, 1, 2]
);

check(
  "closure in a computed key in the for body",
  "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ var o = { [(fs.push(() => i), 'k')]: 1 }; } return fs.map(f => f());",
  [0, 1, 2]
);

check(
  "closure in the inner loop of nested for loops captures both bindings",
  "var fs = []; for (let i = 0; i < 2; i++) { for (let j = 0; j < 2; j++) { /*Y*/ fs.push(() => i * 10 + j); } } return fs.map(f => f());",
  [0, 1, 10, 11]
);

check(
  "closure in a for-of body",
  "var fs = []; for (const v of [1, 2, 3]) { /*Y*/ fs.push(() => v); } return fs.map(f => f());",
  [1, 2, 3]
);

check(
  "closure in a for-of left pattern default",
  "var fs = []; for (const [a, f = () => a] of [[1], [2], [3]]) { /*Y*/ fs.push(f); } return fs.map(f => f());",
  [1, 2, 3]
);

check(
  "closure in a for-in left pattern default",
  "var fs = []; for (const { length, f = () => length } in { ab: 1, cde: 1 }) { /*Y*/ fs.push(f); } return fs.map(f => f());",
  [2, 3]
);

check(
  "closure in a for-in body",
  "var fs = []; for (let k in { a: 1, b: 2 }) { /*Y*/ fs.push(() => k); } return fs.map(f => f());",
  ["a", "b"]
);

check(
  "closure in a for-of iterable expression does not disturb the loop",
  "var seen = []; for (const v of [1, 2, 3].map((x) => x * 2)) { /*Y*/ seen.push(v); } return seen;",
  [2, 4, 6]
);

check(
  "closure in a for-in iterable expression does not disturb the loop",
  "var seen = []; for (const k in Object.fromEntries([['a', 1], ['b', 2]].map((e) => e))) { /*Y*/ seen.push(k); } return seen;",
  ["a", "b"]
);

check(
  "direct eval in a for-of body",
  "var fs = []; for (let v of [1, 2, 3]) { /*Y*/ fs.push(eval('() => v')); } return fs.map(f => f());",
  [1, 2, 3]
);

check(
  "with statement in a for body",
  "var fs = []; for (let i = 0; i < 2; i++) { /*Y*/ with ({}) { fs.push(function () { return i; }); } } return fs.map(f => f());",
  [0, 1]
);

check(
  "a loop that cannot capture still runs its initializer, test and update in order",
  "var log = []; for (let i = (log.push('init'), 0); (log.push('test' + i), i < 2); log.push('update' + i), i++) { /*Y*/ log.push('body' + i); } return log;",
  ["init", "test0", "body0", "update0", "test1", "body1", "update1", "test2"]
);

check(
  "a for-let loop shadows an outer binding and leaves it untouched",
  "var i = 'outer'; var seen = []; for (let i = 0; i < 2; i++) { /*Y*/ seen.push(i); } seen.push(i); return seen;",
  [0, 1, "outer"]
);

check(
  "labeled continue across non-capturing nested for-let loops",
  "var seen = []; outer: for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { /*Y*/ if (j === 1) continue outer; seen.push(i * 10 + j); } } return seen;",
  [0, 10, 20]
);

check(
  "break out of a non-capturing for-of keeps the completion value path intact",
  "var seen = []; for (const v of [1, 2, 3, 4]) { /*Y*/ if (v === 3) break; seen.push(v); } return seen;",
  [1, 2]
);

// Destructuring bindings start uninitialized in every iteration: a binding
// initialized by an earlier iteration must not leak into the next one.
(function () {
  var thrown = [];
  try {
    for (const [a = tdzNameNotDefinedAnywhere, tdzNameNotDefinedAnywhere] of [[1, 5], [undefined, 6]]) {
      thrown.push("iteration " + a);
    }
  } catch (e) {
    thrown.push(e.constructor.name);
  }
  assert.compareArray(thrown, ["iteration 1", "ReferenceError"], "for-of destructuring TDZ per iteration");
})();

// One for-of statement executing re-entrantly must not share state between
// its executions.
(function () {
  function walk(depth) {
    var out = [];
    for (const k of [1, 2]) {
      out.push(k);
      if (depth > 0) {
        out.push(walk(depth - 1).join(""));
      }
    }
    return out;
  }
  assert.sameValue(walk(0).join("|"), "1|2");
  assert.sameValue(walk(1).join("|"), "1|12|2|12");
  assert.sameValue(walk(2).join("|"), "1|112212|2|112212");
})();

// Two generator instances running the same non-capturing loop interleave
// without sharing a frame.
(function () {
  function* counter(n) {
    var s = 0;
    for (let i = 0; i < n; i++) {
      s += i;
      yield s;
    }
    return s;
  }
  var a = counter(4);
  var b = counter(6);
  var seen = [];
  seen.push(a.next().value, b.next().value, a.next().value, b.next().value, a.next().value, b.next().value);
  seen.push(a.next().value, b.next().value, a.next().value, b.next().value);
  assert.compareArray(seen, [0, 0, 1, 1, 3, 3, 6, 6, 6, 10], "interleaved generators");
})();
