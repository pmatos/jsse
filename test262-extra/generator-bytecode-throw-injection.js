/*---
description: >
  Generator.prototype.throw raises its argument at the yield the generator is
  suspended at: enclosing catch blocks handle it, finally blocks run,
  for-of loops close their iterators, and an unhandled error completes the
  generator (sec-generatorresumeabrupt).
esid: sec-generatorresumeabrupt
features: [generators]
---*/

function show(r) {
  return (r.done ? "R:" : "Y:") + String(r.value);
}

var log = [];

// A catch block handles it and the generator continues.
function* catching() {
  try { yield 1; } catch (e) { log.push("caught " + e); yield "after-catch"; }
  yield "end";
}
var it = catching();
it.next();
assert.sameValue(show(it.throw("E")), "Y:after-catch");
assert.sameValue(show(it.next()), "Y:end");
assert.sameValue(log.join(), "caught E");

// Without a handler the error propagates and the generator completes.
function* unhandled() { yield 1; yield 2; }
it = unhandled();
it.next();
assert.throws(Test262Error, function() { it.throw(new Test262Error()); });
assert.sameValue(show(it.next()), "R:undefined");

// finally runs, then the error continues to propagate.
log = [];
function* finallyRuns() {
  try { yield 1; } finally { log.push("fin"); }
}
it = finallyRuns();
it.next();
assert.throws(Test262Error, function() { it.throw(new Test262Error()); });
assert.sameValue(log.join(), "fin");

// A finally block can swallow the throw by returning.
function* swallow() {
  try { yield 1; } finally { return "swallowed"; }
}
it = swallow();
it.next();
assert.sameValue(show(it.throw(new Test262Error())), "R:swallowed");

// A yield in the catch block, then a rethrow.
log = [];
function* rethrow() {
  try { yield 1; } catch (e) { yield "c:" + e; throw "re:" + e; }
}
it = rethrow();
it.next();
assert.sameValue(show(it.throw("X")), "Y:c:X");
var caught;
try { it.next(); } catch (e) { caught = e; }
assert.sameValue(caught, "re:X");

// A throw into an expression-position yield discards the pending operands.
function* inExpression() {
  try { return [1, 2, yield "mid"]; } catch (e) { return "caught " + e; }
}
it = inExpression();
it.next();
assert.sameValue(show(it.throw("T")), "R:caught T");

// A throw into a for-of body closes the iterator, then the handler runs.
log = [];
function* loopBody() {
  try {
    for (const x of { [Symbol.iterator]() { return {
      next() { return { value: 1, done: false }; },
      return() { log.push("close"); return {}; }
    }; } }) { yield x; }
  } catch (e) { log.push("handler " + e); }
}
it = loopBody();
it.next();
assert.sameValue(show(it.throw("L")), "R:undefined");
assert.sameValue(log.join(), "close,handler L");

// Errors thrown by the iterator's return() while closing do not mask the
// original throw.
log = [];
function* closeMasked() {
  try {
    for (const x of { [Symbol.iterator]() { return {
      next() { return { value: 1, done: false }; },
      return() { log.push("close"); throw "close error"; }
    }; } }) { yield x; }
  } catch (e) { log.push("handler " + e); }
}
it = closeMasked();
it.next();
it.throw("orig");
assert.sameValue(log.join(), "close,handler orig");

// An error thrown by the body itself reaches the same handlers.
function* bodyThrows() {
  try { yield 1; throw "body"; } catch (e) { yield "c:" + e; }
}
it = bodyThrows();
it.next();
assert.sameValue(show(it.next()), "Y:c:body");

// The error object keeps its identity.
var err = {};
function* identity() { try { yield 1; } catch (e) { return e; } }
it = identity();
it.next();
assert.sameValue(it.throw(err).value, err);

// A handler inside a nested block scope keeps its bindings.
function* scopes() {
  let outer = "o";
  try {
    let inner = "i";
    yield 1;
  } catch (e) {
    let handled = outer + e;
    yield handled;
  }
}
it = scopes();
it.next();
assert.sameValue(show(it.throw("!")), "Y:o!");
