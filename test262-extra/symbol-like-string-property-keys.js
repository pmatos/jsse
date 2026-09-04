// ECMAScript property keys are either Strings or Symbols, and every String is
// a valid property key. A String that resembles a Symbol's display text must
// remain distinct from the Symbol itself through storage and enumeration.
// Spec: sec-object-type, sec-ordinaryownpropertykeys,
//       sec-enumerableownproperties, sec-getownpropertykeys,
//       sec-serializejsonobject.

function Test262Error(message) {
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};

function assert(condition, message) {
  if (!condition) {
    throw new Test262Error(message);
  }
}

function assertSame(actual, expected, message) {
  if (actual !== expected) {
    throw new Test262Error(message + ": expected " + expected + ", got " + actual);
  }
}

function assertKeys(actual, expected, message) {
  assertSame(actual.length, expected.length, message + " length");
  for (var i = 0; i < expected.length; i++) {
    assertSame(actual[i], expected[i], message + " key " + i);
  }
}

var wellKnownText = "Symbol(Symbol.iterator)";
var asciiText = "Symbol(x)";
var surrogateText = "Symbol(\uD834";
var customSymbol = Symbol("x");

var value = {};
value[wellKnownText] = "well-known text";
value[Symbol.iterator] = "well-known symbol";
value[asciiText] = "ascii text";
value[surrogateText] = "surrogate text";
value[customSymbol] = "custom symbol";

// The exact text formerly used for JSSE's internal well-known Symbol encoding
// must coexist with the actual well-known Symbol key.
assertSame(value[wellKnownText], "well-known text", "well-known text lookup");
assertSame(value[Symbol.iterator], "well-known symbol", "well-known Symbol lookup");
assertSame(value[asciiText], "ascii text", "ASCII text lookup");
assertSame(value[surrogateText], "surrogate text", "surrogate text lookup");
assertSame(value[customSymbol], "custom symbol", "custom Symbol lookup");
assert(wellKnownText in value, "well-known text missing from HasProperty");
assert(Symbol.iterator in value, "well-known Symbol missing from HasProperty");

var names = Object.getOwnPropertyNames(value);
assertKeys(names, [wellKnownText, asciiText, surrogateText], "own property names");

var symbols = Object.getOwnPropertySymbols(value);
assertKeys(symbols, [Symbol.iterator, customSymbol], "own property symbols");

assertKeys(Object.keys(value), names, "Object.keys");
assertKeys(Reflect.ownKeys(value), names.concat(symbols), "Reflect.ownKeys");

var iterated = [];
for (var key in value) {
  iterated.push(key);
}
assertKeys(iterated, names, "for-in");

assertSame(
  Object.getOwnPropertyDescriptor(value, wellKnownText).value,
  "well-known text",
  "string descriptor"
);
assertSame(
  Object.getOwnPropertyDescriptor(value, Symbol.iterator).value,
  "well-known symbol",
  "Symbol descriptor"
);

var jsonValue = JSON.parse(JSON.stringify(value));
assertKeys(Object.keys(jsonValue), names, "JSON keys");
assertSame(jsonValue[wellKnownText], "well-known text", "JSON well-known text");
assertSame(jsonValue[asciiText], "ascii text", "JSON ASCII text");
assertSame(jsonValue[surrogateText], "surrogate text", "JSON surrogate text");

var assigned = Object.assign({}, value);
assertKeys(Reflect.ownKeys(assigned), names.concat(symbols), "Object.assign keys");
assertSame(assigned[wellKnownText], "well-known text", "Object.assign text");
assertSame(assigned[Symbol.iterator], "well-known symbol", "Object.assign Symbol");

var spread = { ...value };
assertKeys(Reflect.ownKeys(spread), names.concat(symbols), "object spread keys");

var trapKeys = [];
var proxy = new Proxy(value, {
  get: function (target, key, receiver) {
    trapKeys.push(key);
    return Reflect.get(target, key, receiver);
  }
});
assertSame(proxy[wellKnownText], "well-known text", "proxy text result");
assertSame(proxy[Symbol.iterator], "well-known symbol", "proxy Symbol result");
assertSame(trapKeys[0], wellKnownText, "proxy received String key");
assertSame(trapKeys[1], Symbol.iterator, "proxy received Symbol key");

var ownKeysProxy = new Proxy({}, {
  ownKeys: function () {
    return [wellKnownText, Symbol.iterator, asciiText, surrogateText, customSymbol];
  },
  getOwnPropertyDescriptor: function () {
    return { configurable: true, enumerable: true, value: 1 };
  }
});
assertKeys(
  Reflect.ownKeys(ownKeysProxy),
  [wellKnownText, Symbol.iterator, asciiText, surrogateText, customSymbol],
  "proxy ownKeys"
);
assertKeys(
  Object.keys(ownKeysProxy),
  [wellKnownText, asciiText, surrogateText],
  "proxy enumerable String keys"
);

assert(delete value[wellKnownText], "delete text key");
assertSame(value[Symbol.iterator], "well-known symbol", "delete text retained Symbol");
assert(delete value[Symbol.iterator], "delete Symbol key");
assertSame(value[asciiText], "ascii text", "delete Symbol retained text");
