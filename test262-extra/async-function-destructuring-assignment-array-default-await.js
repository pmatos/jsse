/*---
description: >
  An await inside an array destructuring-assignment default no longer hangs
  a plain async function: the whole assignment statement runs (including its
  blocking await) and the async function's promise resolves with the bound
  value.
esid: sec-runtime-semantics-keyeddestructuringassignmentevaluation
info: |
  AssignmentElement : DestructuringAssignmentTarget Initializer?

  ...
  3. If Initializer is present and value is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
    b. Set value to ? GetValue(defaultValue).
  ...
  5. Return ? PutValue(lRef, value).
flags: [async]
features: [async-functions, destructuring-assignment]
---*/

async function f() {
  var a;
  [a = await 1] = [];
  return a;
}

f().then(function (v) {
  assert.sameValue(v, 1, 'the awaited default is bound to a');
}).then($DONE, $DONE);
