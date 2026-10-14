// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-json.parse
description: >
  JSON.parse and JSON.rawJSON take the code units of their text argument, so a
  lone surrogate in a string literal or an object key survives, and a code
  point from the private-use range the engine could confuse it with does not
  turn into a surrogate.
features: [json-parse-with-source]
includes: [compareArray.js]
---*/

var L = "\ud800";
var H = "\udc00";

assert.sameValue(JSON.parse('"' + L + '"'), L, "lone high surrogate in a string");
assert.sameValue(JSON.parse('"' + H + '"'), H, "lone low surrogate in a string");
assert.sameValue(JSON.parse('"a' + L + 'b"'), "a" + L + "b", "surrounded by text");
assert.sameValue(JSON.parse('"\\ud800"'), L, "escaped lone surrogate");
assert.sameValue(JSON.parse('"😀"'), "😀", "surrogate pair");
assert.sameValue(JSON.parse('"\ud83d\\ude00"'), "😀", "pair split between literal and escape");

var parsed = JSON.parse('{"' + L + '":1,"a' + H + '":[2,"' + L + '"]}');
assert.compareArray(Object.keys(parsed), [L, "a" + H], "keys keep lone surrogates");
assert.sameValue(parsed["a" + H][1], L, "nested array element");

assert.sameValue(JSON.parse('"\u{F0000}"'), "\u{F0000}", "private-use plane 15 code point");
assert.sameValue(JSON.parse('"\u{F07FF}\ud800"'), "\u{F07FF}" + L, "private-use code point next to a surrogate");

assert.throws(SyntaxError, function() {
  JSON.parse(L);
}, "lone surrogate outside a string");
assert.throws(SyntaxError, function() {
  JSON.parse('{' + L + ':1}');
}, "lone surrogate as an unquoted key");

var seen = [];
JSON.parse('["' + L + '", "\\ud800"]', function(key, value, context) {
  if (key !== "") seen.push(context.source);
  return value;
});
assert.compareArray(seen, ['"' + L + '"', '"\\ud800"'], "reviver context.source");

var raw = JSON.rawJSON('"' + L + '"');
assert.sameValue(raw.rawJSON, '"' + L + '"', "rawJSON text");
assert.sameValue(JSON.stringify(raw), '"' + L + '"', "stringify emits rawJSON text verbatim");
assert.sameValue(JSON.stringify({ k: raw }), '{"k":"' + L + '"}', "stringify nested rawJSON");
assert.throws(SyntaxError, function() {
  JSON.rawJSON(L);
}, "rawJSON of a lone surrogate outside a string");
assert.throws(SyntaxError, function() {
  JSON.rawJSON(" " + L);
}, "rawJSON with leading whitespace");

var genuine = "\u{F0000}\u{F07FF}";
var sources = [];
JSON.parse('["' + genuine + '"]', function(key, value, context) {
  if (key !== "") sources.push(context.source);
  return value;
});
assert.compareArray(sources, ['"' + genuine + '"'], "reviver context.source of private-use code points");
assert.sameValue(JSON.parse('"' + genuine + '"'), genuine, "private-use code points round trip");
var message;
try {
  JSON.parse('"' + genuine + '" x');
} catch (e) {
  message = e.message;
}
assert.sameValue(typeof message, "string", "private-use text followed by junk is a SyntaxError");
