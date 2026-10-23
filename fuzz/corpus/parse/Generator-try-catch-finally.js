// Tests generators with try/catch/finally.
// Spec: ECMAScript 2024, sec-generator-objects

// Basic try/finally in generator
function* genFinally() {
  try {
    yield 1;
    yield 2;
  } finally {
    yield 3;
  }
}

var g = genFinally();
if (g.next().value !== 1) throw new Test262Error('first yield should be 1');
if (g.next().value !== 2) throw new Test262Error('second yield should be 2');
if (g.next().value !== 3) throw new Test262Error('finally yield should be 3');
var last = g.next();
if (last.done !== true) throw new Test262Error('should be done after finally');

// Generator return() triggers finally
function* genReturnFinally() {
  try {
    yield 1;
    yield 2;
  } finally {
    yield "cleanup";
  }
}

var g2 = genReturnFinally();
if (g2.next().value !== 1) throw new Test262Error('first yield should be 1');
var ret = g2.return("early");
if (ret.value !== "cleanup") throw new Test262Error('return() should trigger finally yield, got: ' + ret.value);
var afterCleanup = g2.next();
if (afterCleanup.value !== "early") throw new Test262Error('after finally, return value should complete, got: ' + afterCleanup.value);
if (afterCleanup.done !== true) throw new Test262Error('should be done after return');

// Generator throw() with catch
function* genThrowCatch() {
  try {
    yield 1;
    yield 2;
  } catch (e) {
    yield "caught: " + e;
  }
}

var g3 = genThrowCatch();
g3.next();
var thrown = g3.throw("oops");
if (thrown.value !== "caught: oops") throw new Test262Error('throw should be caught, got: ' + thrown.value);

// try/catch/finally complete path
function* genFull() {
  var log = [];
  try {
    log.push("try");
    yield 1;
    log.push("after-yield");
  } catch (e) {
    log.push("catch:" + e);
  } finally {
    log.push("finally");
  }
  yield log.join(",");
}

var g4 = genFull();
g4.next();
var result = g4.next();
if (result.value !== "try,after-yield,finally") {
  throw new Test262Error('full try/catch/finally path wrong, got: ' + result.value);
}

// yield* delegation
function* inner() {
  yield "a";
  yield "b";
  return "inner-return";
}

function* outer() {
  var result = yield* inner();
  yield "c:" + result;
}

var g5 = outer();
if (g5.next().value !== "a") throw new Test262Error('delegated yield a');
if (g5.next().value !== "b") throw new Test262Error('delegated yield b');
if (g5.next().value !== "c:inner-return") throw new Test262Error('delegation return value');

// yield* with throw forwarding
function* innerThrow() {
  try {
    yield "x";
  } catch (e) {
    yield "caught:" + e;
  }
}

function* outerThrow() {
  yield* innerThrow();
}

var g6 = outerThrow();
g6.next();
var t = g6.throw("err");
if (t.value !== "caught:err") throw new Test262Error('throw forwarding, got: ' + t.value);

// for-of with generator return
function* countUp() {
  var i = 0;
  try {
    while (true) {
      yield i++;
    }
  } finally {
    // finally runs when for-of breaks
  }
}

var collected = [];
for (var v of countUp()) {
  collected.push(v);
  if (v >= 2) break;
}
if (collected.join(",") !== "0,1,2") {
  throw new Test262Error('for-of with break should collect 0,1,2, got: ' + collected.join(","));
}
