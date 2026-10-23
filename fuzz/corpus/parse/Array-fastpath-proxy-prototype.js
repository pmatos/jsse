// The dense-Array indexed-write fast path (issue #125) must not bypass a Proxy
// in the receiver's prototype chain. Per OrdinarySet (sec-ordinaryset /
// sec-ordinarysetwithowndescriptor), when the receiver has no own property for
// the index the engine walks the prototype chain, and a Proxy prototype's
// [[Set]] trap must run. A bare Proxy exposes no own descriptor for the index,
// so a fast path that only checks for an inherited descriptor would skip the
// trap and wrongly create an own element.
// Spec: ECMAScript 2024, sec-ordinaryset,
//       sec-proxy-object-internal-methods-and-internal-slots-set-p-v-receiver.

// Indexed write through a Proxy prototype: the trap runs and NO own element is
// created (the proxy handled the write).
var idxCalls = [];
var a = [];
Object.setPrototypeOf(
  a,
  new Proxy(Array.prototype, { set: function (t, p, v, r) { idxCalls.push(String(p)); return true; } })
);
a[0] = 1;
if (a.length !== 0) {
  throw new Test262Error('idx proxy-proto: expected length 0 (trap handled write), got ' + a.length);
}
if (0 in a || a.hasOwnProperty(0)) {
  throw new Test262Error('idx proxy-proto: index 0 must NOT be an own property');
}
if (idxCalls.length !== 1 || idxCalls[0] !== '0') {
  throw new Test262Error('idx proxy-proto: expected the set trap called once with "0", got ' + JSON.stringify(idxCalls));
}

// Strict-mode write whose proxy-prototype trap returns false must throw TypeError.
(function () {
  'use strict';
  var b = [];
  Object.setPrototypeOf(b, new Proxy(Array.prototype, { set: function () { return false; } }));
  var threw = false;
  try {
    b[0] = 9;
  } catch (e) {
    threw = e instanceof TypeError;
  }
  if (!threw) {
    throw new Test262Error('strict idx proxy-proto set->false: expected TypeError');
  }
})();

// Overwriting an existing own element must NOT consult the prototype Proxy
// (OrdinarySet uses the own data descriptor directly). The fast overwrite
// branch must remain in effect here.
var ovCalls = [];
var c = [10];
Object.setPrototypeOf(c, new Proxy(Array.prototype, { set: function () { ovCalls.push(1); return true; } }));
c[0] = 20;
if (c[0] !== 20 || ovCalls.length !== 0) {
  throw new Test262Error('overwrite own elem must not consult proxy: c[0]=' + c[0] + ' trapCalls=' + ovCalls.length);
}

// Sanity: with an ordinary (non-Proxy) prototype the fast paths still apply.
var d = [1, 2];
d[2] = 3;
if (d.length !== 3 || d[2] !== 3) {
  throw new Test262Error('ordinary-proto append regressed: length=' + d.length + ' d[2]=' + d[2]);
}
if (d.push(4) !== 4 || d.length !== 4 || d[3] !== 4) {
  throw new Test262Error('ordinary-proto push regressed: length=' + d.length + ' d[3]=' + d[3]);
}
