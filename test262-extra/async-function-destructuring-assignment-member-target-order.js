/*---
description: >
  A member-expression target inside an object destructuring-assignment
  pattern has its reference (base, then computed key) evaluated before the
  pattern's own property read, and an await in the computed key suspends the
  async function there rather than after the read.
esid: sec-runtime-semantics-keyeddestructuringassignmentevaluation
info: |
  AssignmentElement : DestructuringAssignmentTarget Initializer?

  1. If DestructuringAssignmentTarget is neither an ObjectLiteral nor an
     ArrayLiteral, then
    a. Let lRef be ? Evaluation of DestructuringAssignmentTarget.
  ...
  3. If DestructuringAssignmentTarget is an ObjectLiteral or an ArrayLiteral,
     then
    ...
  Else,
    a. If Initializer is present and value is undefined, then
      i. Let defaultValue be ? Evaluation of Initializer.
      ii. Set value to ? GetValue(defaultValue).
    b. Return ? PutValue(lRef, value).

  MemberExpression : MemberExpression [ Expression ]
  1. Let baseReference be ? Evaluation of MemberExpression.
  2. Let baseValue be ? GetValue(baseReference).
  3. Return ? EvaluatePropertyAccessWithExpressionKey(baseValue, Expression, strict).
flags: [async]
includes: [compareArray.js]
features: [async-functions, destructuring-assignment, computed-property-names]
---*/

function run(makeFn) {
  var log = [];
  var L = function (x) { log.push(x); };
  var ticks = Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); });
  var p = makeFn(L);
  L('sync-end');
  return Promise.all([p, ticks]).then(function () { return log; });
}

async function memberTargetKeyAwaits(L) {
  var target = {};
  function base() { L('base'); return target; }
  async function key() { L('key-call'); return 'slot'; }
  var source = { get a() { L('get-a'); return 42; } };
  ({ a: base()[await key()] } = source);
  L('slot=' + target.slot);
}

async function memberTargetWithUnusedDefault(L) {
  var target = {};
  function base() { L('base'); return target; }
  var source = { a: 7 };
  ({ a: base()[await 'slot'] = 99 } = source);
  L('slot=' + target.slot);
}

Promise.all([
  run(memberTargetKeyAwaits),
  run(memberTargetWithUnusedDefault),
]).then(function (logs) {
  assert.compareArray(
    logs[0],
    ['base', 'key-call', 'sync-end', 'w1', 'get-a', 'slot=42', 'w2'],
    'the member reference is captured before the await suspends; the property read happens after resuming'
  );
  assert.compareArray(
    logs[1],
    ['base', 'sync-end', 'w1', 'slot=7', 'w2'],
    'a present property is read and stored without evaluating the unused default'
  );
}).then($DONE, $DONE);
