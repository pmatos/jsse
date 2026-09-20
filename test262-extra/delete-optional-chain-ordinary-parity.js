/*---
description: >
  `delete` through an optional chain performs the same [[Delete]] as the plain
  member form for ordinary, Proxy, arguments-exotic and Array objects, and for
  primitive and nullish bases.
info: |
  Runtime Semantics: Evaluation
  UnaryExpression : delete UnaryExpression

  5. If IsPropertyReference(ref) is true, then
    a. Assert: IsPrivateReference(ref) is false.
    b. If IsSuperReference(ref) is true, throw a ReferenceError exception.
    c. Let baseObj be ? ToObject(ref.[[Base]]).
    e. Let deleteStatus be ? baseObj.[[Delete]](ref.[[ReferencedName]]).
    f. If deleteStatus is false and ref.[[Strict]] is true, throw a TypeError
       exception.
    g. Return deleteStatus.

  OptionalExpression only changes how the Reference is produced; a nullish
  base short-circuits the whole chain to undefined, and `delete` of that
  evaluates to true.
esid: sec-delete-operator-runtime-semantics-evaluation
flags: [noStrict]
---*/

var bodies = {
  plain: "delete o[k]",
  optionalBase: "delete o?.[k]",
  optionalDot: "delete o?.k",
  optionalNested: "var h = {a: o}; return delete h?.a.k",
  optionalNestedComputed: "var h = {a: o}; return delete h?.a?.[k]",
};

function build(body, strict) {
  var prefix = strict ? "'use strict'; " : "";
  var stmt = /return/.test(body) ? body + ";" : "return " + body + ";";
  return new Function("o", "k", prefix + stmt);
}

Object.keys(bodies).forEach(function (form) {
  var f = build(bodies[form], false);

  var configurable = { k: 1, other: 2 };
  assert.sameValue(f(configurable, "k"), true, form + ": configurable own property");
  assert.sameValue(Object.prototype.hasOwnProperty.call(configurable, "k"), false, form + ": removed");
  assert.sameValue(configurable.other, 2, form + ": sibling untouched");

  assert.sameValue(f({}, "k"), true, form + ": absent property");

  var fixed = {};
  Object.defineProperty(fixed, "k", { value: 1, configurable: false });
  assert.sameValue(f(fixed, "k"), false, form + ": non-configurable property");
  assert.sameValue(fixed.k, 1, form + ": non-configurable property kept");

  var calls = 0;
  var refusing = new Proxy({}, {
    deleteProperty: function (t, key) {
      calls += 1;
      assert.sameValue(key, "k", form + ": proxy trap key");
      return false;
    },
  });
  assert.sameValue(f(refusing, "k"), false, form + ": proxy trap returning false");
  assert.sameValue(calls, 1, form + ": proxy trap called once (false)");

  calls = 0;
  var accepting = new Proxy({}, {
    deleteProperty: function () {
      calls += 1;
      return true;
    },
  });
  assert.sameValue(f(accepting, "k"), true, form + ": proxy trap returning true");
  assert.sameValue(calls, 1, form + ": proxy trap called once (true)");

  if (bodies[form].indexOf("[k]") !== -1) {
    var arr = [10, 20, 30];
    assert.sameValue(f(arr, "1"), true, form + ": dense array element delete");
    assert.sameValue(1 in arr, false, form + ": dense array element becomes a hole");
    assert.sameValue(arr.length, 3, form + ": array length unchanged");
  }

  var g = build(bodies[form], true);

  var strictConfigurable = { k: 1 };
  assert.sameValue(g(strictConfigurable, "k"), true, form + " (strict): configurable own property");
  assert.sameValue("k" in strictConfigurable, false, form + " (strict): removed");

  assert.throws(TypeError, function () {
    g(fixed, "k");
  }, form + " (strict): non-configurable property");

  calls = 0;
  assert.throws(TypeError, function () {
    g(refusing, "k");
  }, form + " (strict): proxy trap returning false");
  assert.sameValue(calls, 1, form + " (strict): proxy trap called once");

  assert.sameValue(g(accepting, "k"), true, form + " (strict): proxy trap returning true");
});

(function () {
  var mapped = function (a) {
    var result = delete arguments?.[0];
    arguments[0] = "changed";
    return [result, a, arguments[0]];
  };
  var r = mapped("orig");
  assert.sameValue(r[0], true, "mapped arguments: delete result");
  assert.sameValue(r[1], "orig", "mapped arguments: parameter decoupled after delete");
  assert.sameValue(r[2], "changed", "mapped arguments: index re-created as a plain property");
})();

(function () {
  var mapped = function (a) {
    delete arguments[0];
    arguments[0] = "changed";
    return [a, arguments[0]];
  };
  var r = mapped("orig");
  assert.sameValue(r[0], "orig", "plain form: parameter decoupled after delete");
  assert.sameValue(r[1], "changed", "plain form: index re-created as a plain property");
})();

assert.sameValue(new Function("return delete 'str'?.foo;")(), true, "string primitive base, absent key");
assert.sameValue(new Function("return delete (1)?.foo;")(), true, "number primitive base");
assert.sameValue(new Function("return delete true?.foo;")(), true, "boolean primitive base");

assert.sameValue(new Function("return delete null?.foo;")(), true, "null base short-circuits");
assert.sameValue(new Function("return delete undefined?.foo;")(), true, "undefined base short-circuits");
assert.sameValue(new Function("var o = null; return delete o?.a.b;")(), true, "nullish base short-circuits the whole chain");
assert.throws(
  TypeError,
  new Function("var o = {a: undefined}; return delete o?.a.b;"),
  "nullish intermediate reference without ?. still throws"
);
