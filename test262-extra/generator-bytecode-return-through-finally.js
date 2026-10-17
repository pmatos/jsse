/*---
description: >
  Generator.prototype.return resumes a suspended generator with a return
  completion that runs enclosing finally blocks (which may yield, override the
  completion or throw) and closes iterators of enclosing for-of loops, in
  inside-out order (sec-generatorresumeabrupt, sec-try-statement-runtime-semantics-evaluation,
  sec-runtime-semantics-forinofbodyevaluation).
esid: sec-generatorresumeabrupt
features: [generators]
---*/

function show(r) {
  return (r.done ? "R:" : "Y:") + String(r.value);
}

var log = [];

function* simple() {
  try { yield 1; yield 2; } finally { log.push("fin"); }
}
var it = simple();
it.next();
assert.sameValue(show(it.return("x")), "R:x");
assert.sameValue(log.join(), "fin");
assert.sameValue(show(it.next()), "R:undefined");

// The finally block can yield; the return completion resumes afterwards.
log = [];
function* yieldingFinally() {
  try { yield "body"; }
  finally { log.push("f1"); yield "from-finally"; log.push("f2"); }
}
it = yieldingFinally();
it.next();
assert.sameValue(show(it.return("ret")), "Y:from-finally");
assert.sameValue(log.join(), "f1");
assert.sameValue(show(it.next("ignored")), "R:ret");
assert.sameValue(log.join(), "f1,f2");

// A finally block that returns replaces the return completion.
function* overriding() {
  try { yield 1; } finally { return "override"; }
}
it = overriding();
it.next();
assert.sameValue(show(it.return("ret")), "R:override");

// A finally block that throws replaces it with the throw.
function* throwing() {
  try { yield 1; } finally { throw new Test262Error("from finally"); }
}
it = throwing();
it.next();
assert.throws(Test262Error, function() { it.return("ret"); });
assert.sameValue(show(it.next()), "R:undefined");

// A catch block does not intercept a return completion.
log = [];
function* catchIgnored() {
  try { yield 1; } catch (e) { log.push("catch"); } finally { log.push("fin"); }
}
it = catchIgnored();
it.next();
assert.sameValue(show(it.return(5)), "R:5");
assert.sameValue(log.join(), "fin");

// Nested finally blocks run inside-out, each at the yield it suspended at.
log = [];
function* nested() {
  try {
    try { yield "inner"; } finally { log.push("inner-fin"); yield "mid"; log.push("inner-fin2"); }
  } finally { log.push("outer-fin"); yield "outer"; log.push("outer-fin2"); }
}
it = nested();
assert.sameValue(show(it.next()), "Y:inner");
assert.sameValue(show(it.return("done")), "Y:mid");
assert.sameValue(show(it.next()), "Y:outer");
assert.sameValue(show(it.next()), "R:done");
assert.sameValue(log.join(), "inner-fin,inner-fin2,outer-fin,outer-fin2");

// A yield in the middle of an expression: pending operands are discarded.
log = [];
function* inExpression(a) {
  try { return [a, yield "mid", log.push("after")]; } finally { log.push("fin"); }
}
it = inExpression(1);
it.next();
assert.sameValue(show(it.return("x")), "R:x");
assert.sameValue(log.join(), "fin");

// return() closes the iterators of the for-of loops it leaves, innermost first.
log = [];
function iterable(name) {
  return {
    [Symbol.iterator]() {
      var n = 0;
      return {
        next() { return { value: name + n++, done: false }; },
        return(v) { log.push("close " + name); return {}; }
      };
    }
  };
}
function* loops() {
  for (const a of iterable("A")) {
    for (const b of iterable("B")) { yield a + b; }
  }
}
it = loops();
assert.sameValue(show(it.next()), "Y:A0B0");
assert.sameValue(show(it.return("r")), "R:r");
assert.sameValue(log.join(), "close B,close A");

// An iterator whose return() throws: the error propagates (a return completion
// is not a throw completion), and outer iterators are still closed.
log = [];
var thrown = new Test262Error("close failed");
function* closeThrows() {
  for (const a of iterable("A")) {
    for (const b of { [Symbol.iterator]() { return {
      next() { return { value: 1, done: false }; },
      return() { log.push("close B"); throw thrown; }
    }; } }) { yield b; }
  }
}
it = closeThrows();
it.next();
var caught;
try { it.return("r"); } catch (e) { caught = e; }
assert.sameValue(caught, thrown);
assert.sameValue(log.join(), "close B,close A");

// A for-of loop inside a try/finally: iterator closed before the finally runs.
log = [];
function* loopInTry() {
  try {
    for (const a of iterable("A")) { yield a; }
  } finally { log.push("fin"); }
}
it = loopInTry();
it.next();
assert.sameValue(show(it.return(1)), "R:1");
assert.sameValue(log.join(), "close A,fin");

// return() inside a block scope with a closure keeps the binding alive.
function* scoped() {
  let captured = "kept";
  try { yield () => captured; } finally { captured = "changed"; }
}
it = scoped();
var getter = it.next().value;
it.return();
assert.sameValue(getter(), "changed");

// A completed finally does not run twice.
log = [];
function* once() { try { yield 1; } finally { log.push("fin"); } }
it = once();
it.next();
it.return();
it.return();
it.next();
assert.sameValue(log.join(), "fin");

// return() while the generator is suspended in a catch block's try/finally.
log = [];
function* inCatch() {
  try { throw 1; }
  catch (e) { try { yield "in-catch"; } finally { log.push("inner"); } }
  finally { log.push("outer"); }
}
it = inCatch();
assert.sameValue(show(it.next()), "Y:in-catch");
assert.sameValue(show(it.return("z")), "R:z");
assert.sameValue(log.join(), "inner,outer");

// A break out of a for-of whose body yields still closes the iterator once.
log = [];
function* withBreak() {
  outer: for (const a of iterable("A")) {
    for (const b of iterable("B")) {
      yield a + b;
      if (b === "B1") break outer;
    }
  }
  yield "after";
}
it = withBreak();
assert.sameValue(show(it.next()), "Y:A0B0");
assert.sameValue(show(it.next()), "Y:A0B1");
assert.sameValue(show(it.next()), "Y:after");
assert.sameValue(log.join(), "close B,close A");
