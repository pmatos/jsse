/*---
description: Async functions, async generators, for-await-of and using heads keep per-iteration scope semantics when a loop may or may not capture its scope
info: |
  14.7.4.3 ForBodyEvaluation / 14.7.4.4 CreatePerIterationEnvironment
  14.7.5.7 ForIn/OfBodyEvaluation step 5.j (NewDeclarativeEnvironment per iteration)
  14.7.5.7 ForIn/OfBodyEvaluation: DisposeResources of the iteration environment

  A loop whose head, test, update and body create no closure, class, `with`
  or direct `eval` may share one environment across iterations, including when
  the loop suspends on `await`/`yield`. Loops that can capture, and loops with a
  `using` head (whose environment owns a dispose stack), must still get a fresh
  environment per iteration.
esid: sec-forbodyevaluation
includes: [compareArray.js, asyncHelpers.js]
flags: [noStrict, async]
---*/

var AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;

async function runAsyncFunction(source) {
  return new AsyncFunction(source.split("/*Y*/").join("await 0;"))();
}

async function runAsyncGenerator(source) {
  var it = new AsyncGeneratorFunction(source.split("/*Y*/").join("yield 0;"))();
  var step = await it.next();
  while (!step.done) {
    step = await it.next();
  }
  return step.value;
}

async function check(name, source, expected) {
  assert.compareArray(await runAsyncFunction(source), expected, name + " (async function)");
  assert.compareArray(await runAsyncGenerator(source), expected, name + " (async generator)");
}

asyncTest(async function () {
  await check(
    "closure in the for head initializer sees the frozen initial binding",
    "var fs = []; for (let i = 0, f = () => i; i < 3; i++) { /*Y*/ fs.push(f); } return fs.map(f => f());",
    [0, 0, 0]
  );

  await check(
    "closure in the for body",
    "var fs = []; for (let i = 0; i < 3; i++) { /*Y*/ fs.push(() => i); } return fs.map(f => f());",
    [0, 1, 2]
  );

  await check(
    "non-capturing for-let loop across suspensions",
    "var seen = []; for (let i = 0; i < 3; i++) { /*Y*/ seen.push(i); } return seen;",
    [0, 1, 2]
  );

  await check(
    "closure in a for-of body across suspensions",
    "var fs = []; for (const v of [1, 2, 3]) { /*Y*/ fs.push(() => v); } return fs.map(f => f());",
    [1, 2, 3]
  );

  await check(
    "non-capturing for-of loop across suspensions",
    "var seen = []; for (const v of [1, 2, 3]) { /*Y*/ seen.push(v); } return seen;",
    [1, 2, 3]
  );

  await check(
    "non-capturing for-in loop across suspensions",
    "var seen = []; for (const k in { a: 1, b: 2 }) { /*Y*/ seen.push(k); } return seen;",
    ["a", "b"]
  );

  await check(
    "for-await-of over an async iterable with a non-capturing body",
    "async function* src() { yield 1; yield 2; yield 3; } var seen = []; for await (const v of src()) { /*Y*/ seen.push(v); } return seen;",
    [1, 2, 3]
  );

  await check(
    "for-await-of with a closure in the body",
    "async function* src() { yield 1; yield 2; } var fs = []; for await (const v of src()) { /*Y*/ fs.push(() => v); } return fs.map(f => f());",
    [1, 2]
  );

  await check(
    "using head disposes its resource at the end of every iteration",
    "var log = []; var mk = (n) => ({ [Symbol.dispose]() { log.push('d' + n); } }); for (using r of [mk(1), mk(2)]) { /*Y*/ log.push('body'); } return log;",
    ["body", "d1", "body", "d2"]
  );
});
