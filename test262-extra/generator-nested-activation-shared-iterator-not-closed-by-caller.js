/*---
description: >
  A nested generator that iterates the same iterator object as its caller's
  for-of loop, finishes that loop, and then suspends inside a for-of over its
  own iterator does not hand that iterator to the caller: the caller's return()
  closes only the caller's own iterator.
esid: sec-generator.prototype.return
info: |
  GeneratorResumeAbrupt resumes only the receiver's execution context, so only
  the iterators held by the receiver's own for-of loops are closed by return().

  ForIn/OfBodyEvaluation: the loop's iterator is closed by IteratorClose only
  when its own loop exits abruptly.
includes: [compareArray.js]
features: [generators]
---*/

var log = [];
function tracked(name) {
  var i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { return { value: ++i, done: false }; },
    return() { log.push('close:' + name); return {}; },
  };
}

var shared = tracked('shared');
var other = tracked('other');

function* nested() {
  for (var a of shared) {
    if (a < 0) yield a;
    break;
  }
  for (var b of other) yield b;
}

function* outer() {
  for (var x of shared) {
    var n = nested();
    n.next();
    yield x;
  }
}

var o = outer();
o.next();
assert.compareArray(log, ['close:shared'], 'nested break closed the shared iterator');

o.return();
assert.compareArray(
  log,
  ['close:shared', 'close:shared'],
  'outer return() closed only its own iterator, not the one the nested generator is suspended in'
);
