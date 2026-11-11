/*---
description: Array and TypedArray joining preserves preparation, live reads, locale invocation and collecting conversions
esid: sec-joinindexedproperties
includes: [compareArray.js]
features: [TypedArray, BigInt, resizable-arraybuffer]
---*/

var log = [];
var source = Object.create({ 1: "inherited" });
Object.defineProperty(source, "length", {
  get: function () { log.push("length"); return 4; }
});
Object.defineProperty(source, "0", {
  get: function () { log.push("0"); source[2] = "changed"; return null; }
});
var separator = {
  toString: function () { log.push("separator"); return "\uD800"; }
};
assert.sameValue(Array.prototype.join.call(source, separator),
  "\uD800inherited\uD800changed\uD800");
assert.compareArray(log, ["length", "separator", "0"]);

var emptySeparatorCalls = 0;
var emptySeparator = {
  toString: function () { emptySeparatorCalls++; return "!"; }
};
assert.sameValue([].join(emptySeparator), "");
assert.sameValue(new Uint8Array(0).join(emptySeparator), "");
assert.sameValue(emptySeparatorCalls, 2);

var marker = {};
var throwing = { toString: function () { throw marker; } };
try {
  [throwing].join();
  throw new Test262Error("missing conversion error");
} catch (error) {
  assert.sameValue(error, marker);
}
assert.throws(TypeError, function () { [Symbol()].join(); });

var locales = { tag: "locales" };
var options = { tag: "options" };
var calls = 0;
var element = {
  get toLocaleString() {
    assert.sameValue(this, element);
    return function (l, o) {
      "use strict";
      assert.sameValue(this, element);
      assert.sameValue(arguments.length, 2);
      assert.sameValue(l, locales);
      assert.sameValue(o, options);
      calls++;
      return { toString: function () { return "localized"; } };
    };
  }
};
assert.sameValue([undefined, element, null, , element].toLocaleString(
  locales, options, "ignored"), ",localized,,,localized");
assert.sameValue(calls, 2);

var originalNumberLocale = Number.prototype.toLocaleString;
var originalBigIntLocale = BigInt.prototype.toLocaleString;
try {
  Number.prototype.toLocaleString = function (l, o) {
    "use strict";
    assert.sameValue(typeof this, "number");
    assert.sameValue(arguments.length, 2);
    assert.sameValue(l, undefined);
    assert.sameValue(o, undefined);
    return "n" + this;
  };
  BigInt.prototype.toLocaleString = function (l, o) {
    "use strict";
    assert.sameValue(typeof this, "bigint");
    assert.sameValue(arguments.length, 2);
    assert.sameValue(l, undefined);
    assert.sameValue(o, undefined);
    return "b" + this;
  };
  assert.sameValue([1, 2].toLocaleString(), "n1,n2");
  assert.sameValue(new Uint8Array([1, 2]).toLocaleString(), "n1,n2");
  assert.sameValue(new Float64Array([1, 2]).toLocaleString(), "n1,n2");
  assert.sameValue(new BigInt64Array([1n, 2n]).toLocaleString(), "b1,b2");

  var buffer = new ArrayBuffer(3);
  var typed = new Uint8Array(buffer);
  typed.set([1, 2, 3]);
  Number.prototype.toLocaleString = function () {
    "use strict";
    $262.detachArrayBuffer(buffer);
    return "n" + this;
  };
  assert.sameValue(typed.toLocaleString(), "n1,,");
} finally {
  Number.prototype.toLocaleString = originalNumberLocale;
  BigInt.prototype.toLocaleString = originalBigIntLocale;
}

var detached = new Uint8Array(2);
var coercions = 0;
$262.detachArrayBuffer(detached.buffer);
assert.throws(TypeError, function () {
  detached.join({ toString: function () { coercions++; return "|"; } });
});
assert.sameValue(coercions, 0);
assert.throws(TypeError, function () {
  Uint8Array.prototype.join.call({}, emptySeparator);
});
assert.sameValue(emptySeparatorCalls, 2);

var detachOnSeparator = new Uint8Array([1, 2, 3]);
assert.sameValue(detachOnSeparator.join({
  toString: function () {
    $262.detachArrayBuffer(detachOnSeparator.buffer);
    return "|";
  }
}), "||");

var resizable = new ArrayBuffer(3, { maxByteLength: 6 });
var tracking = new Uint8Array(resizable);
tracking.set([1, 2, 3]);
assert.sameValue(tracking.join({
  toString: function () { resizable.resize(1); return "|"; }
}), "1||");

// Ephemeral elements and locale results must survive collection during their
// getters, calls and ToString. Run through public methods in both evaluators.
var collecting = {
  get length() { $262.gc(); return 2; },
  get 0() {
    return {
      get toLocaleString() {
        $262.gc();
        return function (l, o) {
          $262.gc();
          assert.sameValue(l.tag, "locales");
          assert.sameValue(o.tag, "options");
          return { toString: function () { $262.gc(); return "live"; } };
        };
      }
    };
  },
  get 1() {
    return { toLocaleString: function () { $262.gc(); return "last"; } };
  }
};
assert.sameValue(Array.prototype.toLocaleString.call(collecting,
  { tag: "locales" }, { tag: "options" }), "live,last");
assert.sameValue(Array.prototype.join.call({
  get length() { $262.gc(); return 2; },
  get 0() { return { toString: function () { $262.gc(); return "first"; } }; },
  get 1() { return "last"; }
}, { toString: function () { $262.gc(); return "|"; } }), "first|last");
