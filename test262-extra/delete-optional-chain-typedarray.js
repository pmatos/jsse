/*---
description: >
  `delete` through an optional chain applies the TypedArray exotic [[Delete]]
  exactly like the plain member form.
info: |
  [[Delete]] ( P )  (TypedArray exotic objects)

  1. If P is a String, then
    a. Let numericIndex be CanonicalNumericIndexString(P).
    b. If numericIndex is not undefined, then
      i. If IsValidIntegerIndex(O, numericIndex) is false, return true;
         else return false.
  2. Return ! OrdinaryDelete(O, P).

  Runtime Semantics: Evaluation
  UnaryExpression : delete UnaryExpression

  5.e. Let deleteStatus be ? baseObj.[[Delete]](ref.[[ReferencedName]]).
    f. If deleteStatus is false and ref.[[Strict]] is true, throw a TypeError
       exception.
    g. Return deleteStatus.
esid: sec-typedarray-delete
includes: [testTypedArray.js, detachArrayBuffer.js]
flags: [noStrict]
features: [TypedArray]
---*/

var bodies = {
  plain: "return delete ta[k];",
  optionalBase: "return delete ta?.[k];",
  optionalNested: "var o = {ta: ta}; return delete o?.ta[k];",
  optionalNestedOptional: "var o = {ta: ta}; return delete o?.ta?.[k];",
};

function build(body, strict) {
  return new Function("ta", "k", (strict ? "'use strict'; " : "") + body);
}

testWithTypedArrayConstructors(function (TA) {
  Object.keys(bodies).forEach(function (form) {
    var sloppy = build(bodies[form], false);
    var strict = build(bodies[form], true);
    var label = TA.name + " " + form;

    var ta = new TA(2);
    assert.sameValue(sloppy(ta, "0"), false, label + ": valid index 0");
    assert.sameValue(sloppy(ta, "1"), false, label + ": valid index 1");
    assert.sameValue(sloppy(ta, 0), false, label + ": valid index as number key");
    assert.sameValue(ta.length, 2, label + ": length unchanged");
    assert.sameValue(ta[0], 0, label + ": element intact");
    assert.throws(TypeError, function () {
      strict(ta, "0");
    }, label + ": valid index throws in strict mode");
    assert.throws(TypeError, function () {
      strict(ta, "1");
    }, label + ": valid index 1 throws in strict mode");

    ["2", "-1", "1.5", "-0", "Infinity", "-Infinity", "NaN", "1e21"].forEach(function (k) {
      assert.sameValue(sloppy(ta, k), true, label + ": canonical numeric non-index " + k);
      assert.sameValue(strict(ta, k), true, label + ": canonical numeric non-index " + k + " (strict)");
    });

    assert.sameValue(sloppy(ta, "01"), true, label + ": non-canonical numeric string 01");
    assert.sameValue(sloppy(ta, "foo"), true, label + ": absent non-numeric key");

    ta.foo = 1;
    assert.sameValue(sloppy(ta, "foo"), true, label + ": ordinary own property deleted");
    assert.sameValue("foo" in ta, false, label + ": ordinary own property removed");

    var detached = new TA(2);
    $DETACHBUFFER(detached.buffer);
    assert.sameValue(sloppy(detached, "0"), true, label + ": detached buffer, index 0");
    assert.sameValue(strict(detached, "0"), true, label + ": detached buffer, index 0 (strict)");
  });
});

var nullish = new Function("var o = null; return delete o?.ta[0];");
assert.sameValue(nullish(), true, "nullish base short-circuits");
