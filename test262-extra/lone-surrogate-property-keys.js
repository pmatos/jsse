// ECMAScript String values are arbitrary UTF-16 code-unit sequences, and all
// String values are valid property keys. In particular, lone surrogates must
// not be replaced with U+FFFD while evaluating, storing, looking up, or
// enumerating a property key.
// Spec: sec-ecmascript-language-types-string-type, sec-object-type,
//       sec-topropertykey, sec-runtime-semantics-propertydefinitionevaluation,
//       sec-ordinaryownpropertykeys.

function Test262Error(message) {
  this.message = message || "";
}

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

function assertKey(actual, expectedCodeUnit, message) {
  assertSame(actual.length, 1, message + " length");
  assertSame(actual.charCodeAt(0), expectedCodeUnit, message + " code unit");
}

var replacement = "\uFFFD";
var high = "\uD834";
var low = "\uDF06";

var value = {
  "\uFFFD": "replacement",
  "\uD834": "high",
  "\uDF06": "low"
};

assertSame(value[replacement], "replacement", "replacement lookup");
assertSame(value[high], "high", "high-surrogate lookup");
assertSame(value[low], "low", "low-surrogate lookup");
assert(high in value, "high surrogate missing from HasProperty");
assert(low in value, "low surrogate missing from HasProperty");
assert(value.hasOwnProperty(high), "high surrogate missing from HasOwnProperty");
assert(value.hasOwnProperty(low), "low surrogate missing from HasOwnProperty");

var keys = Object.keys(value);
assertSame(keys.length, 3, "Object.keys length");
assertKey(keys[0], 0xFFFD, "Object.keys replacement key");
assertKey(keys[1], 0xD834, "Object.keys high-surrogate key");
assertKey(keys[2], 0xDF06, "Object.keys low-surrogate key");

var names = Object.getOwnPropertyNames(value);
assertSame(names.length, 3, "Object.getOwnPropertyNames length");
assertKey(names[0], 0xFFFD, "getOwnPropertyNames replacement key");
assertKey(names[1], 0xD834, "getOwnPropertyNames high-surrogate key");
assertKey(names[2], 0xDF06, "getOwnPropertyNames low-surrogate key");

var ownKeys = Reflect.ownKeys(value);
assertSame(ownKeys.length, 3, "Reflect.ownKeys length");
assertKey(ownKeys[1], 0xD834, "Reflect.ownKeys high-surrogate key");
assertKey(ownKeys[2], 0xDF06, "Reflect.ownKeys low-surrogate key");

var entries = Object.entries(value);
assertKey(entries[1][0], 0xD834, "Object.entries high-surrogate key");
assertSame(entries[1][1], "high", "Object.entries high-surrogate value");
assertSame(Object.values(value).join(","), "replacement,high,low", "Object.values");

var iterated = [];
for (var iteratedKey in value) {
  iterated.push(iteratedKey.charCodeAt(0));
}
assertSame(iterated.join(","), "65533,55348,57094", "for-in keys");

var descriptor = Object.getOwnPropertyDescriptor(value, high);
assertSame(descriptor.value, "high", "descriptor lookup");

value[high] = "changed";
assertSame(value[high], "changed", "assignment");
assertSame(value[replacement], "replacement", "assignment kept replacement distinct");
assert(delete value[high], "delete high surrogate");
assert(!value.hasOwnProperty(high), "deleted high surrogate remained present");
value[high] = "re-added";
keys = Object.keys(value);
assertKey(keys[2], 0xD834, "delete and re-add ordering");

var computed = { [high]: 1, [low]: 2, [replacement]: 3 };
assertSame(Object.keys(computed).length, 3, "computed key count");
assertSame(computed[high], 1, "computed high-surrogate lookup");
assertSame(computed[low], 2, "computed low-surrogate lookup");

var spilled = { a: 0, b: 1, c: 2, d: 3, [replacement]: 4, [high]: 5, [low]: 6 };
assertSame(Object.keys(spilled).length, 7, "spilled property-map key count");
assertSame(spilled[replacement], 4, "spilled replacement key");
assertSame(spilled[high], 5, "spilled high-surrogate key");
assertSame(spilled[low], 6, "spilled low-surrogate key");

var assigned = Object.assign({}, computed);
assertSame(Object.keys(assigned).length, 3, "Object.assign key count");
assertSame(assigned[high], 1, "Object.assign high-surrogate key");
var spread = { ...computed };
assertSame(spread[low], 2, "object spread low-surrogate key");
var { [high]: extracted, ...rest } = computed;
assertSame(extracted, 1, "destructuring high-surrogate key");
assertSame(Object.keys(rest).length, 2, "object rest key count");
assertSame(rest[low], 2, "object rest low-surrogate key");

var descriptors = Object.getOwnPropertyDescriptors(computed);
assertSame(descriptors[high].value, 1, "getOwnPropertyDescriptors high-surrogate key");
var defined = Object.defineProperties({}, descriptors);
assertSame(defined[high], 1, "defineProperties high-surrogate key");

var classKey = class { [high] = 7; [low]() { return 8; } };
var classValue = new classKey();
assertSame(classValue[high], 7, "class field high-surrogate key");
assertSame(classValue[low](), 8, "class method low-surrogate key");
assertKey(classValue[low].name, 0xDF06, "class method name");

var namedFunctions = {
  [high]: function() {},
  get [low]() { return 1; }
};
assertKey(namedFunctions[high].name, 0xD834, "inferred function name");
var getterName = Object.getOwnPropertyDescriptor(namedFunctions, low).get.name;
assertSame(getterName.length, 5, "getter name length");
assertSame(getterName.slice(0, 4), "get ", "getter name prefix");
assertSame(getterName.charCodeAt(4), 0xDF06, "getter name key code unit");

var json = JSON.stringify(computed);
var parsed = JSON.parse(json);
assertSame(Object.keys(parsed).length, 3, "JSON round-trip key count");
assertSame(parsed[high], 1, "JSON round-trip high-surrogate key");

var trapKey;
var proxy = new Proxy({}, {
  defineProperty: function(target, key, desc) {
    trapKey = key;
    return Reflect.defineProperty(target, key, desc);
  },
  get: function(target, key, receiver) {
    trapKey = key;
    return Reflect.get(target, key, receiver);
  }
});
Object.defineProperty(proxy, high, { value: 42, configurable: true });
assertKey(trapKey, 0xD834, "proxy defineProperty trap key");
assertSame(proxy[high], 42, "proxy get result");
assertKey(trapKey, 0xD834, "proxy get trap key");

var ownKeysProxy = new Proxy(computed, {
  ownKeys: function() { return [replacement, high, low]; }
});
assertKey(Reflect.ownKeys(ownKeysProxy)[1], 0xD834, "proxy ownKeys high-surrogate key");

var frozen = { [high]: 1, [replacement]: 2 };
Object.freeze(frozen);
assert(Object.isFrozen(frozen), "Object.freeze with lone-surrogate key");
assertSame(frozen[high], 1, "Object.freeze kept high-surrogate key distinct");
