// A function's own properties must behave as ordinary own data properties no
// matter which operation touches the function object first: delete, define,
// freeze, preventExtensions, key enumeration, descriptor reads, `in`, Proxy
// forwarding and Function.prototype.bind.
// Spec: sec-ordinarydelete, sec-ordinarydefineownproperty,
//       sec-setintegritylevel, sec-ordinaryownpropertykeys,
//       sec-proxy-object-internal-methods-and-internal-slots-ownpropertykeys,
//       sec-function.prototype.bind (HasOwnProperty + Get of "length", Get of
//       "name"), sec-forin-and-for-of-statements.

function Test262Error(message) {
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};

function assert(cond, label) {
  if (!cond) throw new Test262Error(label);
}

function assertKeys(actual, expected, label) {
  var a = JSON.stringify(actual);
  var e = JSON.stringify(expected);
  assert(a === e, label + ": expected " + e + ", got " + a);
}

function mk() {
  var captured = 1;
  return function target(a, b) { return captured + a + b; };
}

// delete as the first operation.
var f = mk();
assert(delete f.name, "delete f.name");
assert(!f.hasOwnProperty("name"), "name deleted");
assert(f.name === "", "name falls back to Function.prototype.name");
assert(f.length === 2, "length untouched");
assert(f.prototype.constructor === f, "prototype untouched");
var keys = Reflect.ownKeys(f).filter(function (k) { return k === "length" || k === "name" || k === "prototype"; });
assertKeys(keys, ["length", "prototype"], "keys after delete name");

f = mk();
assert(Reflect.deleteProperty(f, "prototype") === false, "prototype is non-configurable");
assert(delete f.length, "delete f.length");
assert(f.length === 0, "length falls back to Function.prototype.length");

// delete, then re-add appends at the end.
f = mk();
delete f.name;
Object.defineProperty(f, "name", { value: "again", configurable: true });
keys = Reflect.ownKeys(f).filter(function (k) { return k === "length" || k === "name" || k === "prototype"; });
assertKeys(keys, ["length", "prototype", "name"], "re-added name goes last");

// defineProperty as the first operation.
f = mk();
Object.defineProperty(f, "length", { value: 42 });
assert(f.length === 42, "length redefined");
var d = Object.getOwnPropertyDescriptor(f, "length");
assert(d.writable === false && d.enumerable === false && d.configurable === true, "length attrs preserved");
assert(f.name === "target", "name intact");
assertKeys(Reflect.ownKeys(f).slice(0, 2), ["length", "name"], "order after define length");

f = mk();
Object.defineProperty(f, "prototype", { value: { custom: true } });
assert(f.prototype.custom === true, "prototype redefined");
assert(new f().custom === true, "new uses redefined prototype");

f = mk();
Object.defineProperty(f, "extra", { value: 1, enumerable: true, configurable: true, writable: true });
keys = Reflect.ownKeys(f);
assert(keys.indexOf("length") < keys.indexOf("name") && keys.indexOf("name") < keys.indexOf("prototype"),
  "builtin keys keep order: " + JSON.stringify(keys));
assert(keys.indexOf("prototype") < keys.indexOf("extra"), "added key follows builtin keys");

// Assignment to non-writable `name` / `length` fails; `prototype` is writable.
f = mk();
assert(Reflect.set(f, "name", "x") === false, "name is non-writable");
assert(f.name === "target", "name unchanged");
f.prototype = {};
assert(typeof f.prototype === "object", "prototype is writable");

// freeze / preventExtensions as the first operation.
f = mk();
Object.freeze(f);
assert(Object.isFrozen(f), "frozen");
d = Object.getOwnPropertyDescriptor(f, "prototype");
assert(d.writable === false && d.configurable === false, "frozen prototype attrs");
d = Object.getOwnPropertyDescriptor(f, "name");
assert(d.configurable === false, "frozen name attrs");
assert(f.name === "target" && f.length === 2, "values after freeze");
assert(Object.isFrozen(f.prototype) === false, "freeze is shallow");

f = mk();
Object.preventExtensions(f);
assert(!Object.isExtensible(f), "not extensible");
assert(f.hasOwnProperty("name") && f.hasOwnProperty("length") && f.hasOwnProperty("prototype"),
  "own keys survive preventExtensions");
assert(Reflect.defineProperty(f, "fresh", { value: 1 }) === false, "cannot add after preventExtensions");
assert(delete f.name, "can still delete configurable name");
assert(Reflect.defineProperty(f, "name", { value: "x" }) === false, "cannot re-add name after preventExtensions");

f = mk();
Object.seal(f);
assert(Object.isSealed(f), "sealed");

// Descriptor / existence reads as the first operation.
f = mk();
var descs = Object.getOwnPropertyDescriptors(f);
assert("length" in descs && "name" in descs && "prototype" in descs, "descriptors present");
assert(descs.length.value === 2 && descs.name.value === "target", "descriptor values");

f = mk();
assert("name" in f && "length" in f && "prototype" in f, "`in` sees own keys");
assert(f.hasOwnProperty("name"), "hasOwnProperty name");
assert(!f.propertyIsEnumerable("name"), "name not enumerable");

f = mk();
var seen = [];
for (var k in f) seen.push(k);
assertKeys(seen, [], "for-in over a bare function");
assertKeys(Object.keys(f), [], "Object.keys over a bare function");
f.visible = 1;
seen = [];
for (k in f) seen.push(k);
assertKeys(seen, ["visible"], "for-in sees only added enumerable keys");

// Proxies of functions.
f = mk();
var p = new Proxy(f, {});
keys = Reflect.ownKeys(p);
assert(keys.indexOf("length") >= 0 && keys.indexOf("name") >= 0 && keys.indexOf("prototype") >= 0,
  "proxy ownKeys: " + JSON.stringify(keys));
assert(keys.indexOf("length") < keys.indexOf("name"), "proxy key order");
assert(p.name === "target" && p.length === 2, "proxy get");
assert(p(1, 2) === 4, "proxy call");

f = mk();
var trapKeys;
p = new Proxy(f, {
  ownKeys: function (t) { trapKeys = Reflect.ownKeys(t); return trapKeys; },
});
Object.getOwnPropertyNames(p);
assert(trapKeys.indexOf("prototype") >= 0, "ownKeys trap sees target's keys");

// bind reads length (HasOwnProperty + Get) and name (Get) even when mutated first.
f = mk();
Object.defineProperty(f, "length", { value: 10 });
var bf = f.bind(null, 1, 2);
assert(bf.length === 8, "bind length from redefined length: " + bf.length);
assert(bf.name === "bound target", "bind name");

f = mk();
delete f.length;
bf = f.bind(null);
assert(bf.length === 0, "bind length when own length deleted: " + bf.length);

f = mk();
Object.defineProperty(f, "name", { value: 5, configurable: true });
bf = f.bind();
assert(bf.name === "bound ", "bind name with non-string name: " + JSON.stringify(bf.name));

// Class constructors.
class K { static s() {} }
assert(delete K.name, "class name configurable");
assert(K.hasOwnProperty("name") === false, "class name deleted");
assert(Reflect.deleteProperty(K, "prototype") === false, "class prototype non-configurable");
assert(Reflect.defineProperty(K, "prototype", { value: {} }) === false, "class prototype not redefinable");
assertKeys(Reflect.ownKeys(K), ["length", "prototype", "s"], "class keys after delete name");
