/*---
description: >
  The Annex B initializer of a `for (var x = init in obj)` head is evaluated
  exactly once, before the RHS, even when the loop body suspends a generator.
esid: sec-initializers-in-forin-statement-heads
info: |
  ForInOfStatement : for ( var BindingIdentifier Initializer in Expression )
  Statement
  1. Let binding be ? Evaluation of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(...).
  3. Let value be ? Evaluation of Initializer.
  4. Perform ? PutValue(lhs, value).
  5. Return ? ForIn/OfLoopEvaluation of the rest of the statement.
flags: [noStrict]
includes: [compareArray.js]
features: [generators]
---*/

var log = [];
function trace(name, value) {
  log.push(name);
  return value;
}

function* g(o) {
  for (var i = trace('init', 'seed') in trace('rhs', o)) {
    yield i;
  }
  yield i;
}
assert.compareArray([...g({ a: 1, b: 2 })], ['a', 'b', 'b'], 'keys replace the initializer value');
assert.compareArray(log, ['init', 'rhs'], 'initializer runs once, before the RHS');

log = [];
function* nullishRhs() {
  for (var i = trace('init', 'seed') in null) {
    yield 'body';
  }
  yield i;
}
assert.compareArray([...nullishRhs()], ['seed'], 'initializer still applied when the RHS is nullish');
assert.compareArray(log, ['init'], 'initializer ran exactly once');

function* suspendingInit() {
  for (var i = yield 'init' in { p: 1 }) {
    yield i;
  }
}
var it = suspendingInit();
assert.sameValue(it.next().value, 'init', 'initializer suspends before the loop');
assert.sameValue(it.next('sent').value, 'p', 'loop then runs over the RHS');
