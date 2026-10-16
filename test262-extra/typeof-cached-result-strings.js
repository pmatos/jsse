/*---
description: >
  The eight typeof result strings are shared constants: each has the exact
  value required by the spec table, and building a longer string from one
  (concatenation, += in a loop) never alters later typeof results.
info: |
  The typeof Operator, Runtime Semantics: Evaluation (13.5.3.1), Table 41:
  typeof Operator Results.
esid: sec-typeof-operator-runtime-semantics-evaluation
features: [Symbol, BigInt, class, Proxy]
---*/

assert.sameValue(typeof undefined, "undefined");
assert.sameValue(typeof null, "object");
assert.sameValue(typeof true, "boolean");
assert.sameValue(typeof 1, "number");
assert.sameValue(typeof "s", "string");
assert.sameValue(typeof Symbol(), "symbol");
assert.sameValue(typeof 1n, "bigint");
assert.sameValue(typeof {}, "object");
assert.sameValue(typeof function () {}, "function");
assert.sameValue(typeof class {}, "function");
assert.sameValue(typeof new Proxy(function () {}, {}), "function");
assert.sameValue(typeof new Proxy({}, {}), "object");

function inFunction(o) {
  var u;
  return [
    typeof u, typeof null, typeof true, typeof 1, typeof "s", typeof Symbol(),
    typeof 1n, typeof o, typeof inFunction
  ];
}
assert.sameValue(
  inFunction({}).join(),
  "undefined,object,boolean,number,string,symbol,bigint,object,function"
);

if (typeof $262 !== "undefined" && $262.IsHTMLDDA) {
  assert.sameValue(typeof $262.IsHTMLDDA, "undefined");
  assert.sameValue(typeof new Proxy($262.IsHTMLDDA, {}), "object");
}

assert.sameValue(typeof undeclared_cached_typeof_binding, "undefined");
assert.throws(ReferenceError, function () {
  typeof tdzBinding;
  let tdzBinding;
});

// Extending a typeof result must not change the shared strings.
for (var i = 0; i < 4; i++) {
  var t = typeof i;
  t += "-suffix";
  var u = typeof i;
  u = u + "!";
  assert.sameValue(t, "number-suffix");
  assert.sameValue(u, "number!");
  assert.sameValue(typeof i, "number");
}
var s = typeof 1;
var s1 = s + "a";
var s2 = s + "b";
assert.sameValue(s1, "numbera");
assert.sameValue(s2, "numberb");
assert.sameValue(s, "number");
assert.sameValue(typeof 2, "number");
assert.sameValue(typeof "x" === "string", true);

assert.sameValue(typeof typeof 1, "string");
assert.sameValue(typeof typeof typeof 1, "string");
assert.sameValue(typeof Object(typeof 1), "object");
assert.sameValue((typeof 1).length, 6);
