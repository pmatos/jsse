// Array.prototype.push and the other Array mutators that go through the generic
// Set(O, P, V, true) helper (obj_set_throw) must not bypass a Proxy in the
// receiver's prototype chain. Per OrdinarySet (sec-ordinaryset /
// sec-ordinarysetwithowndescriptor), when the receiver has no own property for
// the key the engine walks the prototype chain, and a Proxy prototype's [[Set]]
// trap must run with the original receiver. A bare Proxy exposes no own
// descriptor, so a helper that only consults an inherited descriptor would skip
// the trap and wrongly create an own property (issue #166).
// Spec: ECMAScript 2024, sec-array.prototype.push (step Set(O, ..., E, true)),
//       sec-ordinaryset,
//       sec-proxy-object-internal-methods-and-internal-slots-set-p-v-receiver.

// push into a fresh array whose prototype is a Proxy: the set trap handles the
// element write, so NO own element is created (only length, which is the
// array's own property, is bumped).
var calls = [];
var a = [];
Object.setPrototypeOf(
  a,
  new Proxy(Array.prototype, { set: function (t, p, v, r) { calls.push(String(p)); return true; } })
);
a.push(1);
if (a.length !== 1) {
  throw new Test262Error('push proxy-proto: expected length 1, got ' + a.length);
}
if (0 in a || a.hasOwnProperty(0)) {
  throw new Test262Error('push proxy-proto: index 0 must NOT be an own property (trap handled it)');
}
if (calls.length !== 1 || calls[0] !== '0') {
  throw new Test262Error('push proxy-proto: expected set trap once with "0", got ' + JSON.stringify(calls));
}

// push appends at index === old length; that index has no own property, so the
// proxy trap must run for it.
var calls2 = [];
var b = [0, 0, 0];
Object.setPrototypeOf(
  b,
  new Proxy(Array.prototype, { set: function (t, p, v, r) { calls2.push(String(p)); return true; } })
);
b.push(7);
if (b.length !== 4 || (3 in b) || b.hasOwnProperty(3)) {
  throw new Test262Error('push proxy-proto append: index 3 must not be own; len=' + b.length);
}
if (calls2.length !== 1 || calls2[0] !== '3') {
  throw new Test262Error('push proxy-proto append: expected set trap once with "3", got ' + JSON.stringify(calls2));
}

// push whose proxy-prototype trap returns false must throw TypeError, because
// push performs Set(O, P, V, true) (the Throw flag is set regardless of strict
// mode).
var threw = false;
var c = [];
Object.setPrototypeOf(c, new Proxy(Array.prototype, { set: function () { return false; } }));
try {
  c.push(9);
} catch (e) {
  threw = e instanceof TypeError;
}
if (!threw) {
  throw new Test262Error('push proxy-proto set->false: expected TypeError');
}

// fill writes to indices that already exist as own data properties; OrdinarySet
// uses the own descriptor directly and must NOT consult the prototype Proxy.
var fillCalls = [];
var d = [1, 2, 3];
Object.setPrototypeOf(d, new Proxy(Array.prototype, { set: function () { fillCalls.push(1); return true; } }));
d.fill(9);
if (fillCalls.length !== 0 || d[0] !== 9 || d[1] !== 9 || d[2] !== 9) {
  throw new Test262Error('fill own indices must not consult proxy: arr=' + JSON.stringify(Array.from(d)) + ' trapCalls=' + fillCalls.length);
}

// Sanity: with an ordinary (non-Proxy) prototype, push behaves normally.
var e = [1, 2];
if (e.push(4) !== 3 || e.length !== 3 || e[2] !== 4) {
  throw new Test262Error('ordinary-proto push regressed: ret/len/last = ' + e.push + '/' + e.length + '/' + e[2]);
}
