/*---
description: >
  Assigning to an Array's "length" property through a path that reaches
  ArraySetLength via the engine's internal [[DefineOwnProperty]] dispatch
  (e.g. a destructuring assignment target) must not be affected by
  properties inherited from Object.prototype. The internal descriptor built
  for this operation only ever carries [[Value]]; it must never be
  round-tripped through an ordinary JS object (which inherits from
  Object.prototype) and back, since doing so lets an unrelated
  Object.prototype mutation leak into ToPropertyDescriptor's validation.
info: |
  jsse issue #417/#416 fix, PR #442: a follow-up review comment found that
  the new ArraySetLength dispatch path built a JS descriptor object via
  FromPropertyDescriptor (whose [[Prototype]] is Object.prototype) and then
  read it back via ToPropertyDescriptor (which uses [[HasProperty]], walking
  the prototype chain) before calling ArraySetLength. A non-callable
  Object.prototype.get therefore caused a plain length assignment to
  incorrectly throw "Getter must be a function".
esid: sec-arraysetlength
---*/

Object.defineProperty(Object.prototype, 'get', {
  value: 1,
  configurable: true,
});

var a = [1, 2, 3];
({ length: a.length } = { length: 1 });

if (a.length !== 1) {
  throw new Test262Error('destructuring-assigned a.length must become 1, got ' + a.length);
}
if (a[1] !== undefined) {
  throw new Test262Error('elements at or beyond the new length must be removed');
}

delete Object.prototype.get;
