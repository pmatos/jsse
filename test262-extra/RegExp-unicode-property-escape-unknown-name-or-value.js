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

var binaryProperties = [
  "ASCII", "ASCII_Hex_Digit", "AHex", "Alphabetic", "Alpha", "Any", "Assigned",
  "Bidi_Control", "Bidi_C", "Bidi_Mirrored", "Bidi_M", "Case_Ignorable", "CI",
  "Cased", "Changes_When_Casefolded", "CWCF", "Changes_When_Casemapped",
  "CWCM", "Changes_When_Lowercased", "CWL", "Changes_When_NFKC_Casefolded",
  "CWKCF", "Changes_When_Titlecased", "CWT", "Changes_When_Uppercased", "CWU",
  "Dash", "Default_Ignorable_Code_Point", "DI", "Deprecated", "Dep",
  "Diacritic", "Dia", "Emoji", "Emoji_Component", "EComp", "Emoji_Modifier",
  "EMod", "Emoji_Modifier_Base", "EBase", "Emoji_Presentation", "EPres",
  "Extended_Pictographic", "ExtPict", "Extender", "Ext", "Grapheme_Base",
  "Gr_Base", "Grapheme_Extend", "Gr_Ext", "Hex_Digit", "Hex",
  "IDS_Binary_Operator", "IDSB", "IDS_Trinary_Operator", "IDST", "ID_Continue",
  "IDC", "ID_Start", "IDS", "Ideographic", "Ideo", "Join_Control", "Join_C",
  "Logical_Order_Exception", "LOE", "Lowercase", "Lower", "Math",
  "Noncharacter_Code_Point", "NChar", "Pattern_Syntax", "Pat_Syn",
  "Pattern_White_Space", "Pat_WS", "Quotation_Mark", "QMark", "Radical",
  "Regional_Indicator", "RI", "Sentence_Terminal", "STerm", "Soft_Dotted",
  "SD", "Terminal_Punctuation", "Term", "Unified_Ideograph", "UIdeo",
  "Uppercase", "Upper", "Variation_Selector", "VS", "White_Space", "space",
  "XID_Continue", "XIDC", "XID_Start", "XIDS"
];

binaryProperties.forEach(function (name) {
  ["u", "v"].forEach(function (flag) {
    new RegExp("\\p{" + name + "}", flag);
    new RegExp("\\P{" + name + "}", flag);
    new RegExp("[\\p{" + name + "}]", flag);
  });
});

assert(/\p{Bidi_Mirrored}/u.test("("), "Bidi_Mirrored matches (");
assert(!/\p{Bidi_M}/u.test("a"), "Bidi_M does not match a");

assert(/\p{Script=Latin}/u.test("a"), "Script=Latin matches a");
assert(!/\p{Script=Latin}/u.test("1"), "Script=Latin does not match 1");
assert(/[\p{scx=Grek}]/v.test("α"), "scx=Grek matches alpha");
assert(/[\p{Lu}--\p{ASCII}]/v.test("À"), "Lu minus ASCII matches A-grave");
