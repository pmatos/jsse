// Copyright (C) 2026 the JSSE authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-patterns-static-semantics-early-errors
description: >
  An unknown property name or property value in a \p{...} / \P{...} escape is
  an early SyntaxError in u and v mode, inside and outside character classes
  (including nested classes and ClassSetOperands), for Script and
  Script_Extensions values as well as General_Category values.
info: |
  Static Semantics: Early Errors

  UnicodePropertyValueExpression :: UnicodePropertyName = UnicodePropertyValue
    It is a Syntax Error if the source text matched by UnicodePropertyName is
    not a Unicode property name or property alias listed in the "Property name
    and aliases" column of Table 69.
    It is a Syntax Error if the source text matched by UnicodePropertyValue is
    not a property value or property value alias for the Unicode property or
    property alias given by the source text matched by UnicodePropertyName
    listed in PropertyValueAliases.txt.

  UnicodePropertyValueExpression :: LoneUnicodePropertyNameOrValue
    It is a Syntax Error if the source text matched by
    LoneUnicodePropertyNameOrValue is not a Unicode property value or property
    value alias for the General_Category (gc) property listed in
    PropertyValueAliases.txt, nor a binary property or binary property alias
    listed in Table 70, nor a binary property of strings listed in Table 71.
---*/

var cases = [
  // v mode, inside classes
  ["[\\p{Foo}]", "v"],
  ["[\\P{Foo}]", "v"],
  ["[^\\p{Foo}]", "v"],
  ["[[\\p{Foo}]]", "v"],
  ["[\\p{Lu}--\\p{Foo}]", "v"],
  ["[\\p{Foo}--\\p{Lu}]", "v"],
  ["[a&&\\p{Foo}]", "v"],
  ["[\\p{Lu}&&\\p{Foo}]", "v"],
  ["[\\p{gc=Nope}]", "v"],
  ["[\\p{Lu=Lu}]", "v"],
  ["[\\p{ASCII=Yes}]", "v"],
  ["[\\p{Script}]", "v"],
  ["[\\p{General_Category}]", "v"],
  ["[\\p{Script=Nope}]", "v"],
  ["[\\P{Script=Nope}]", "v"],
  ["[\\p{sc=Nope}]", "v"],
  ["[\\p{scx=Nope}]", "v"],
  ["[\\p{Script_Extensions=Nope}]", "v"],
  ["[\\p{Script=latin}]", "v"],
  ["[\\p{sc=GREEK}]", "v"],
  ["[^\\p{Script=Nope}]", "v"],
  ["[[\\p{Script=Nope}]]", "v"],
  ["[\\p{Lu}--\\p{Script=Nope}]", "v"],
  // u and v mode, outside classes
  ["\\p{Foo}", "u"],
  ["\\p{Foo}", "v"],
  ["\\p{gc=Nope}", "u"],
  ["\\p{Script=Nope}", "u"],
  ["\\p{Script=Nope}", "v"],
  ["\\P{Script=Nope}", "u"],
  ["\\p{sc=Nope}", "u"],
  ["\\p{sc=Nope}", "v"],
  ["\\p{scx=Nope}", "u"],
  ["\\p{scx=Nope}", "v"],
  ["\\p{Script_Extensions=Nope}", "u"],
  ["\\p{Script=latin}", "u"],
  ["\\p{Script=latin}", "v"],
  ["\\p{sc=GREEK}", "u"],
  ["\\p{scx=grek}", "u"],
  // u mode, inside classes
  ["[\\p{Foo}]", "u"],
  ["[\\p{Script=Nope}]", "u"],
  ["[\\P{Script=Nope}]", "u"],
  ["[^\\p{scx=Nope}]", "u"],
  ["[\\p{Script=latin}]", "u"]
];

cases.forEach(function (c) {
  assert.throws(SyntaxError, function () {
    new RegExp(c[0], c[1]);
  }, "/" + c[0] + "/" + c[1]);
});

var valid = [
  ["[\\p{Lu}]", "v"],
  ["[\\P{Lu}]", "v"],
  ["[\\p{Script=Latin}]", "v"],
  ["[\\p{Script=Unknown}]", "v"],
  ["[\\p{scx=Grek}]", "v"],
  ["[\\p{Basic_Emoji}]", "v"],
  ["[\\p{Lu}--\\p{ASCII}]", "v"],
  ["[a&&\\p{Script=Latin}]", "v"],
  ["\\p{Script=Latin}", "u"],
  ["\\p{sc=Zzzz}", "u"],
  ["[\\p{Script_Extensions=Greek}]", "u"],
  ["\\p{Basic_Emoji}", "v"]
];

valid.forEach(function (c) {
  new RegExp(c[0], c[1]);
});

assert(/\p{Script=Latin}/u.test("a"), "Script=Latin matches a");
assert(!/\p{Script=Latin}/u.test("1"), "Script=Latin does not match 1");
assert(/[\p{scx=Grek}]/v.test("α"), "scx=Grek matches alpha");
assert(/[\p{Lu}--\p{ASCII}]/v.test("À"), "Lu minus ASCII matches A-grave");
