/*---
description: A for-in/of/await-of identifier or destructuring assignment target is resolved and written with PutValue each iteration, so a let/const still in its TDZ or an immutable binding throws
info: |
  14.7.5.7 Runtime Semantics: ForIn/OfBodyEvaluation
    If destructuring is false, let lhsRef be Completion(Evaluation of lhs)
    and, for an assignment, PutValue(lhsRef, nextValue)
  6.2.5.6 PutValue ( V, W ) -> SetMutableBinding
  9.1.1.1.5 SetMutableBinding ( N, V, S )
    If the binding for N is an uninitialized binding, throw a ReferenceError.
    If the binding is immutable and S is true (or it is a const), throw a TypeError.
  9.1.1.4.5 SetMutableBinding (global env): unresolvable reference in strict code throws a ReferenceError
esid: sec-for-in-and-for-of-statements
flags: [async, noStrict]
includes: [asyncHelpers.js]
---*/

function tdzForIn() {
  for (t in { a: 1 }) ;
  let t = 0;
}
assert.throws(ReferenceError, tdzForIn, "for-in with a let in its TDZ");

function tdzForInConst() {
  for (t in { a: 1 }) ;
  const t = 0;
}
assert.throws(ReferenceError, tdzForInConst, "for-in with a const in its TDZ");

function tdzForInClass() {
  for (t in { a: 1 }) ;
  class t {}
}
assert.throws(ReferenceError, tdzForInClass, "for-in with a class binding in its TDZ");

function tdzForOf() {
  for (t of [1]) ;
  let t = 0;
}
assert.throws(ReferenceError, tdzForOf, "for-of with a let in its TDZ");

function tdzForOfArrayPattern() {
  for ([t] of [[1]]) ;
  let t = 0;
}
assert.throws(ReferenceError, tdzForOfArrayPattern, "for-of array pattern target in its TDZ");

function tdzForOfObjectPattern() {
  for ({ a: t } of [{ a: 1 }]) ;
  let t = 0;
}
assert.throws(ReferenceError, tdzForOfObjectPattern, "for-of object pattern target in its TDZ");

function tdzForInArrayPattern() {
  for ([t] in { a: 1 }) ;
  let t = 0;
}
assert.throws(ReferenceError, tdzForInArrayPattern, "for-in array pattern target in its TDZ");

function tdzNoIterations() {
  for (t in {}) ;
  for (t of []) ;
  let t = 0;
  return "ok";
}
assert.sameValue(tdzNoIterations(), "ok", "no iteration performs no PutValue");

function tdzClosedIterator() {
  var closed = 0;
  var iterable = {
    [Symbol.iterator]() {
      return {
        next() { return { done: false, value: 1 }; },
        return() { closed++; return {}; }
      };
    }
  };
  try {
    for (t of iterable) ;
  } catch (e) {
    assert.sameValue(e.constructor, ReferenceError);
  }
  let t = 0;
  return closed;
}
assert.sameValue(tdzClosedIterator(), 1, "for-of closes the iterator when the target PutValue throws");

function afterInit() {
  let t = 0;
  var seen = [];
  for (t in { a: 1, b: 2 }) seen.push(t);
  for (t of [3, 4]) seen.push(t);
  return seen.join() + "|" + t;
}
assert.sameValue(afterInit(), "a,b,3,4|4", "an initialised let is assigned each iteration");

const constTarget = 1;
assert.throws(TypeError, function() {
  for (constTarget in { a: 1 }) ;
}, "for-in const target");
assert.throws(TypeError, function() {
  for (constTarget of [1]) ;
}, "for-of const target");

assert.throws(ReferenceError, function() {
  "use strict";
  for (undeclaredInStrictForIn in { a: 1 }) ;
}, "strict for-in unresolvable target");
assert.throws(ReferenceError, function() {
  "use strict";
  for (undeclaredInStrictForOf of [1]) ;
}, "strict for-of unresolvable target");

(function() {
  for (sloppyImplicitGlobal in { a: 1 }) ;
})();
assert.sameValue(sloppyImplicitGlobal, "a", "sloppy for-in creates an implicit global");

var withObj = { w: 0 };
with (withObj) {
  for (w in { k: 1 }) ;
}
assert.sameValue(withObj.w, "k", "for-in target resolves through a with object");

var holder = { p: 0 };
for (holder.p in { m: 1 }) ;
assert.sameValue(holder.p, "m", "member expression for-in target is unaffected");
for (holder["p"] of [7]) ;
assert.sameValue(holder.p, 7, "member expression for-of target is unaffected");

for (var v1 in { z: 1 }) ;
assert.sameValue(v1, "z", "var for-in head");
for (let l1 in { z: 1 }) ;
for (const c1 of [1]) ;

asyncTest(async function() {
  async function tdzForAwait() {
    for await (t of [1]) ;
    let t = 0;
  }
  await assert.throwsAsync(ReferenceError, tdzForAwait, "for-await-of with a let in its TDZ");

  async function afterInitAwait() {
    let t = 0;
    for await (t of [Promise.resolve(5)]) ;
    return t;
  }
  assert.sameValue(await afterInitAwait(), 5, "for-await-of assigns an initialised let");
});
