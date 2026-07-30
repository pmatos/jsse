/*---
description: >
  A property write on a primitive base (e.g. `sym.toString = 0`) invokes
  [[Set]] on ToObject(base), but the receiver argument passed to [[Set]] stays
  the *original primitive*, never the disposable wrapper. Since the receiver
  is not an Object, OrdinarySetWithOwnDescriptor's "Receiver is not an Object"
  check must reject the write, and in strict mode that rejection must surface
  as a TypeError.
info: |
  jsse issue #417 / #416: a receiver-shadowing bug made this write silently
  succeed on the throwaway wrapper object instead of being rejected, so
  strict mode never threw. A naive fix that corrects only the receiver
  (without also guarding [[Set]]'s prototype-chain-walk fallthrough) would
  regress into something worse: writing the property directly onto whatever
  prototype object happens to be found while walking the chain from the
  primitive's own [[Prototype]] — i.e. mutating shared, engine-wide state.
  This test pins both: the throw, and that the prototype is left untouched.
esid: sec-putvalue
flags: [onlyStrict]
---*/

var sym = Symbol('probe');
var threw = false;
try {
  sym.toString = 0;
} catch (e) {
  threw = e instanceof TypeError;
}
if (!threw) {
  throw new Test262Error('sym.toString = 0 in strict mode must throw TypeError');
}
if (typeof Symbol.prototype.toString !== 'function') {
  throw new Test262Error('Symbol.prototype.toString must remain the native function, not be overwritten');
}

// Same shape, one level deeper: an inherited writable data property reached
// only by walking a user-defined prototype chain from a primitive base.
Number.prototype.probeValue = 1;
var n = 5;
threw = false;
try {
  n.probeValue = 99;
} catch (e) {
  threw = e instanceof TypeError;
}
if (!threw) {
  throw new Test262Error('n.probeValue = 99 in strict mode must throw TypeError');
}
if (Number.prototype.probeValue !== 1) {
  throw new Test262Error('Number.prototype.probeValue must not be mutated by a primitive-receiver write');
}
delete Number.prototype.probeValue;
