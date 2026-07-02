// Several exotic objects are built by first creating an ordinary function and
// then STRIPPING properties the spec forbids them from owning: the Proxy
// constructor has no "prototype" (sec-proxy-constructor), a bound function has
// no own "prototype" (sec-bound-function-exotic-objects), and a method
// definition has no own "caller"/"arguments" (sec-runtime-semantics-
// methoddefinitionevaluation / sec-forbidden-extensions). Stripping must remove
// the key from the ordered key list as well as the property map, or the
// forbidden name leaks back through Object.getOwnPropertyNames / Reflect.ownKeys
// even though hasOwnProperty already reports false.
// Spec: ECMAScript 2024, sec-proxy-constructor, sec-bound-function-exotic-
//       objects, sec-forbidden-extensions, sec-ordinaryownpropertykeys.

function Test262Error(message) {
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};

function assertNoOwn(obj, name, label) {
  if (obj.hasOwnProperty(name)) {
    throw new Test262Error(label + ': unexpectedly has own "' + name + '"');
  }
  var names = Object.getOwnPropertyNames(obj);
  if (names.indexOf(name) !== -1) {
    throw new Test262Error(
      label + ': "' + name + '" leaked into getOwnPropertyNames ' + JSON.stringify(names)
    );
  }
  if (Reflect.ownKeys(obj).indexOf(name) !== -1) {
    throw new Test262Error(label + ': "' + name + '" leaked into Reflect.ownKeys');
  }
}

// Proxy constructor: no own "prototype".
assertNoOwn(Proxy, "prototype", "Proxy constructor");

// Bound function: no own "prototype".
function target() {}
var bound = target.bind(null);
assertNoOwn(bound, "prototype", "bound function");

// Object-literal method: no own "caller"/"arguments".
var obj = { method() {} };
assertNoOwn(obj.method, "caller", "object method");
assertNoOwn(obj.method, "arguments", "object method");

// Shorthand/computed method definitions behave the same way.
var key = "computed";
var obj2 = { [key]() {}, "quoted-name"() {} };
assertNoOwn(obj2.computed, "caller", "computed method");
assertNoOwn(obj2["quoted-name"], "arguments", "quoted method");
