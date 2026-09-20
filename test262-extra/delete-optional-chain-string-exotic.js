/*---
description: >
  `delete` through an optional chain applies the String exotic object's
  non-configurable index and `length` properties exactly like the plain member
  form, for both String objects and (via ToObject) primitive strings.
info: |
  String exotic objects: [[GetOwnProperty]] ( P ) reports each in-range index
  property with [[Configurable]]: false, and the `length` own property is
  { [[Writable]]: false, [[Enumerable]]: false, [[Configurable]]: false }, so
  OrdinaryDelete returns false for them.

  Runtime Semantics: Evaluation
  UnaryExpression : delete UnaryExpression

  5.c. Let baseObj be ? ToObject(ref.[[Base]]).
    e. Let deleteStatus be ? baseObj.[[Delete]](ref.[[ReferencedName]]).
    f. If deleteStatus is false and ref.[[Strict]] is true, throw a TypeError
       exception.
esid: sec-string-exotic-objects
flags: [noStrict]
---*/

var bodies = {
  plain: "return delete s[k];",
  optionalBase: "return delete s?.[k];",
  optionalNested: "var o = {s: s}; return delete o?.s[k];",
  optionalNestedOptional: "var o = {s: s}; return delete o?.s?.[k];",
};

function build(body, strict) {
  return new Function("s", "k", (strict ? "'use strict'; " : "") + body);
}

Object.keys(bodies).forEach(function (form) {
  var sloppy = build(bodies[form], false);
  var strict = build(bodies[form], true);

  [new String("ab"), "ab"].forEach(function (s) {
    var label = form + " " + (typeof s === "string" ? "primitive" : "wrapper");

    ["0", "1", "length"].forEach(function (k) {
      assert.sameValue(sloppy(s, k), false, label + ": delete " + k);
      assert.throws(TypeError, function () {
        strict(s, k);
      }, label + ": delete " + k + " (strict)");
    });
    assert.sameValue(sloppy(s, 1), false, label + ": numeric key 1");

    ["2", "01", "-1", "foo"].forEach(function (k) {
      assert.sameValue(sloppy(s, k), true, label + ": delete out-of-range or non-index " + k);
      assert.sameValue(strict(s, k), true, label + ": delete " + k + " (strict)");
    });
  });

  var wrapper = new String("ab");
  wrapper.x = 1;
  assert.sameValue(sloppy(wrapper, "x"), true, form + ": added own property deleted");
  assert.sameValue("x" in wrapper, false, form + ": added own property removed");
  assert.sameValue(wrapper.length, 2, form + ": length intact");
  assert.sameValue(wrapper[0], "a", form + ": index intact");
});
