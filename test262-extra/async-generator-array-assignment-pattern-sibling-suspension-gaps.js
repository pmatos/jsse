// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-iteratordestructuringassignmentevaluation
description: >
  When one element of an array-assignment pattern contains a suspending
  `await`, the whole pattern lowers through the state-machine transform
  (`lower_array_pattern_assignment`). Sibling elements that have no
  suspension of their own must still behave exactly as an ordinary
  (non-lowered) array-assignment pattern would: a default must actually be
  conditionally applied (not silently dropped), and a MemberExpression
  target's reference (and a rest target's reference) must be evaluated
  before the iterator is stepped/drained, per
  `IteratorDestructuringAssignmentEvaluation`'s step order. A MemberExpression
  rest target whose own computed key suspends must also genuinely suspend
  the enclosing async generator, the same as any other `await` reachable
  through this pattern.
info: |
  AssignmentElement : DestructuringAssignmentTarget Initializer?

  1. If DestructuringAssignmentTarget is neither an ObjectLiteral nor an
     ArrayLiteral, then
    a. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  2. Let value be undefined.
  3. If iteratorRecord.[[Done]] is false, then
    a. Let next be ? IteratorStepValue(iteratorRecord).
    [...]
  4. If Initializer is present and value is undefined, then
    [...]
    b. Let defaultValue be ? Evaluation of Initializer.
    c. Let v be ? GetValue(defaultValue).
  [...]
  7. Return ? PutValue(lRef, v).

  AssignmentRestElement : ... DestructuringAssignmentTarget

  1. If DestructuringAssignmentTarget is neither an ObjectLiteral nor an
     ArrayLiteral, then
    a. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  2. Let A be ! ArrayCreate(0).
  [...the iterator is drained into A...]
  [...]
  5. Return ? PutValue(lRef, A).

  NOTE: Left to right evaluation order is maintained by evaluating a
  DestructuringAssignmentTarget that is not a destructuring pattern prior to
  accessing the iterator or evaluating the Initializer.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [async-iteration, destructuring-assignment]
---*/

function flush() {
  var p = Promise.resolve();
  for (var i = 0; i < 20; i++) p = p.then(function () {});
  return p;
}

asyncTest(async function () {
  // A non-suspending element with a default, next to a suspending sibling:
  // the default must still be conditionally applied, not silently dropped.
  var a1, b1;
  async function g1() {
    [a1 = 1, b1 = await 2] = [undefined, 20];
    return [a1, b1];
  }
  var r1 = await g1();
  assert.sameValue(r1[0], 1, 'the default fires when the stepped value is undefined');
  assert.sameValue(r1[1], 20, 'the sibling default is skipped when the stepped value is defined');

  var a2, b2;
  async function g2() {
    [a2 = 1, b2 = await 2] = [7, 20];
    return [a2, b2];
  }
  var r2 = await g2();
  assert.sameValue(r2[0], 7, 'the default is skipped when the stepped value is defined');
  assert.sameValue(r2[1], 20, 'the sibling default is skipped when the stepped value is defined');

  // An anonymous function/class default on an identifier target, next to a
  // suspending sibling, must be named after the target (NamedEvaluation),
  // not after whatever internal temp holds the stepped value while this
  // pattern is lowered.
  var h, cls, arrow, y1;
  async function g3named() {
    [h = function () {}, arrow = () => {}, cls = class {}, y1 = await 1] = [];
    return [h.name, arrow.name, cls.name];
  }
  var names = await g3named();
  assert.sameValue(names[0], 'h', 'an anonymous function default is named after its identifier target');
  assert.sameValue(names[1], 'arrow', 'an anonymous arrow default is named after its identifier target');
  assert.sameValue(names[2], 'cls', 'an anonymous class default is named after its identifier target');

  // The same default on a MemberExpression target must stay unnamed --
  // NamedEvaluation never applies when the target is not an identifier
  // reference -- and, critically, must not pick up the internal temp's name
  // either.
  var obj1 = {}, y2;
  async function g3unnamed() {
    [obj1.p = function () {}, y2 = await 1] = [];
    return obj1.p.name;
  }
  assert.sameValue(await g3unnamed(), '', "a member-expression target's default stays unnamed");

  // A non-suspending MemberExpression element next to a suspending sibling:
  // its reference must be captured before the iterator is stepped for that
  // element, exactly as it would be if the whole pattern had no suspension.
  var log = [];
  var target = {};
  function getTarget() {
    log.push('getTarget');
    return target;
  }
  var iterable = {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() {
          log.push('next' + i);
          return { done: i >= 2, value: i++ };
        },
      };
    },
  };
  var c;
  async function g3() {
    [getTarget().x, c = await 1] = iterable;
  }
  await g3();
  assert.sameValue(
    log.join(','),
    'getTarget,next0,next1',
    'the member-expression reference is captured before its own Step, even though a later sibling (not this element) is what forces the lowering'
  );
  assert.sameValue(target.x, 0, 'the element value is still written to the captured reference');

  // A MemberExpression rest target next to a suspending sibling: its
  // reference must be captured before the iterator is drained -- but after
  // the earlier, non-rest element's own Step, since that element is
  // processed first regardless of which element actually suspends.
  var log2 = [];
  var restTarget = {};
  function getRestTarget() {
    log2.push('getRestTarget');
    return restTarget;
  }
  var callCount2 = 0;
  var iterable2 = {
    [Symbol.iterator]() {
      return {
        next() {
          log2.push('next' + callCount2);
          if (callCount2 === 0) {
            callCount2++;
            return { done: false, value: 'A' };
          }
          if (callCount2 === 1) {
            callCount2++;
            return { done: false, value: 'B' };
          }
          callCount2++;
          return { done: true, value: undefined };
        },
      };
    },
  };
  var d;
  async function g4() {
    [d = await 1, ...getRestTarget().rest] = iterable2;
  }
  await g4();
  assert.sameValue(
    log2.join(','),
    'next0,getRestTarget,next1,next2',
    'the non-rest element is stepped first, then the rest reference is captured before the drain begins'
  );
  assert.compareArray(restTarget.rest, ['B'], 'the drained values (excluding the element already consumed) are written to the captured reference');

  // A MemberExpression rest target whose own computed key suspends must
  // genuinely suspend the enclosing async generator at that `await` -- a
  // concurrent `.return()` must only enqueue, not settle early, the same
  // guarantee this PR establishes for every other shape.
  var obj = {};
  async function* g5() {
    [...obj[await new Promise(function () {})]] = [1, 2, 3];
    yield 'unreachable';
  }
  var gen = g5();
  var nextSettled = false;
  var returnSettled = false;
  gen.next().then(
    function () { nextSettled = true; },
    function () { nextSettled = true; }
  );
  gen.return('done-value').then(
    function () { returnSettled = true; },
    function () { returnSettled = true; }
  );
  await flush();
  assert.sameValue(nextSettled, false, 'the in-flight next() stays pending behind the stuck Await in a rest target key');
  assert.sameValue(
    returnSettled,
    false,
    '.return() during a pending Await inside a rest target computed key only enqueues -- it must not settle early'
  );

  // A nested array- or object-pattern *target* (not just its default) that
  // itself contains a suspension, sitting next to a non-suspending sibling,
  // must still genuinely suspend -- the identifier-only NamedEvaluation fast
  // path above must not also swallow this shape.
  async function* g6() {
    var a;
    [[a = await new Promise(function () {})] = [], b] = [undefined, 1];
    yield 'unreachable';
  }
  var gen6 = g6();
  var nextSettled6 = false;
  var returnSettled6 = false;
  gen6.next().then(
    function () { nextSettled6 = true; },
    function () { nextSettled6 = true; }
  );
  gen6.return('done-value').then(
    function () { returnSettled6 = true; },
    function () { returnSettled6 = true; }
  );
  await flush();
  assert.sameValue(nextSettled6, false, 'the in-flight next() stays pending behind the stuck Await nested inside an array-pattern target');
  assert.sameValue(
    returnSettled6,
    false,
    '.return() during a pending Await nested inside an array-pattern target only enqueues -- it must not settle early'
  );

  async function* g7() {
    var a;
    [{ a = await new Promise(function () {}) } = {}, b] = [undefined, 1];
    yield 'unreachable';
  }
  var gen7 = g7();
  var nextSettled7 = false;
  var returnSettled7 = false;
  gen7.next().then(
    function () { nextSettled7 = true; },
    function () { nextSettled7 = true; }
  );
  gen7.return('done-value').then(
    function () { returnSettled7 = true; },
    function () { returnSettled7 = true; }
  );
  await flush();
  assert.sameValue(nextSettled7, false, 'the in-flight next() stays pending behind the stuck Await nested inside an object-pattern target');
  assert.sameValue(
    returnSettled7,
    false,
    '.return() during a pending Await nested inside an object-pattern target only enqueues -- it must not settle early'
  );
});
