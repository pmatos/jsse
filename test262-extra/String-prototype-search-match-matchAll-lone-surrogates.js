// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-string.prototype.match
description: >
  String.prototype.search, match and matchAll convert the receiver and a
  non-RegExp argument with ToString and hand their code units, lone
  surrogates included, to RegExpCreate and the RegExp method, so distinct
  lone surrogates are not confused with each other or with U+FFFD.
includes: [compareArray.js]
---*/

var L = "\ud800";
var H = "\ud801";
var R = "�";

assert.sameValue(("a" + L + "b").search(L), 1, "search finds the surrogate");
assert.sameValue(("a" + H + "b").search(L), -1, "search does not match a different lone surrogate");
assert.sameValue(("a" + R + "b").search(L), -1, "search does not match U+FFFD");
assert.sameValue(("a" + L + "b").search(R), -1, "U+FFFD does not match a lone surrogate");

var m = ("a" + L + "b").match(L);
assert.notSameValue(m, null, "match result");
assert.sameValue(m[0], L, "match[0]");
assert.sameValue(m.index, 1, "match index");
assert.sameValue(m.input, "a" + L + "b", "match input keeps the receiver code units");
assert.sameValue(("a" + H).match(L), null, "match of a different lone surrogate");
assert.sameValue(("a" + R).match(L), null, "match does not treat U+FFFD as the surrogate");

var all = Array.from(("x" + L + H + L).matchAll(L), function(r) { return r.index; });
assert.compareArray(all, [1, 3], "matchAll indices");
assert.sameValue(Array.from(("x" + H).matchAll(L)).length, 0, "matchAll misses a different surrogate");
assert.sameValue(Array.from(("x" + L).matchAll(L))[0][0], L, "matchAll value");
assert.sameValue(Array.from(("x" + L).matchAll(L))[0].input, "x" + L, "matchAll input");

var viaObject = {
  toString: function() { return L; }
};
assert.sameValue(("a" + L).search(viaObject), 1, "search with a ToString-coerced pattern");
