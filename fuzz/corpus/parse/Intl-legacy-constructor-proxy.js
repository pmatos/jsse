// ECMA-402 legacy-constructor: Proxy-receiver behavior of ChainNumberFormat /
// ChainDateTimeFormat (not covered by test262's intl-normative-optional files).
// Spec: ChainNumberFormat / ChainDateTimeFormat use
//   ? OrdinaryHasInstance(%C%, this) and ? DefinePropertyOrThrow(this, ...),
// so abrupt completions must propagate and the define must honor the
// receiver's [[DefineOwnProperty]] (firing a Proxy's defineProperty trap).

// --- DefinePropertyOrThrow goes through the receiver's [[DefineOwnProperty]] ---
// A Proxy chain receiver must see its defineProperty trap fire with the
// fallback symbol, and the chained formatter must then be observable via Unwrap.
var defineKeys = [];
var nfTarget = Object.create(Intl.NumberFormat.prototype);
var nfProxy = new Proxy(nfTarget, {
  defineProperty: function (t, key, desc) {
    defineKeys.push(key);
    return Reflect.defineProperty(t, key, desc);
  }
});
var nfResult = Intl.NumberFormat.call(nfProxy);
if (nfResult !== nfProxy) {
  throw new Test262Error('chain must return the receiver, got a different value');
}
if (defineKeys.length !== 1) {
  throw new Test262Error(
    'defineProperty trap must fire exactly once, fired ' + defineKeys.length + ' times');
}
if (typeof defineKeys[0] !== 'symbol' ||
    defineKeys[0].description !== 'IntlLegacyConstructedSymbol') {
  throw new Test262Error('defineProperty trap key must be the fallback symbol');
}
// Unwrap reads the fallback symbol via the ordinary [[Get]]; because the define
// went through the trap, the chained formatter is reachable on the target.
var nfOpts = Intl.NumberFormat.prototype.resolvedOptions.call(nfProxy);
if (typeof nfOpts !== 'object' || typeof nfOpts.locale !== 'string') {
  throw new Test262Error('resolvedOptions via chained Proxy receiver must succeed');
}

// --- OrdinaryHasInstance abrupt completion propagates (revoked Proxy) ---
function expectThrows(fn, ctor, msg) {
  var threw = false;
  try {
    fn();
  } catch (e) {
    threw = true;
    if (!(e instanceof ctor)) {
      throw new Test262Error(msg + ' — wrong error type: ' + e);
    }
  }
  if (!threw) {
    throw new Test262Error(msg + ' — no error thrown');
  }
}

var nfRev = Proxy.revocable(Object.create(Intl.NumberFormat.prototype), {});
nfRev.revoke();
expectThrows(
  function () { Intl.NumberFormat.call(nfRev.proxy); },
  TypeError,
  'Intl.NumberFormat.call(revokedProxy) must rethrow, not return a fresh formatter');

var dtfRev = Proxy.revocable(Object.create(Intl.DateTimeFormat.prototype), {});
dtfRev.revoke();
expectThrows(
  function () { Intl.DateTimeFormat.call(dtfRev.proxy); },
  TypeError,
  'Intl.DateTimeFormat.call(revokedProxy) must rethrow, not return a fresh formatter');

// --- A throwing getPrototypeOf trap propagates its own exception ---
var sentinel = new RangeError('from getPrototypeOf');
var throwingProxy = new Proxy(Object.create(Intl.NumberFormat.prototype), {
  getPrototypeOf: function () { throw sentinel; }
});
var got;
try {
  Intl.NumberFormat.call(throwingProxy);
  throw new Test262Error('throwing getPrototypeOf during has-instance must propagate');
} catch (e) {
  got = e;
}
if (got !== sentinel) {
  throw new Test262Error('the getPrototypeOf trap exception must propagate unchanged, got: ' + got);
}
