// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-string.prototype.localecompare
description: >
  localeCompare, like Intl.Collator.prototype.compare, is a consistent
  comparator over all Strings: distinct lone surrogates compare unequal, in
  the same direction either way round, and equal Strings compare equal.
---*/

var sign = function(n) { return n < 0 ? -1 : n > 0 ? 1 : 0; };
var collator = new Intl.Collator();

var samples = ["", "a", "b", "�", "\ud800", "\ud801", "\udc00", "a\ud800", "a\ud801", "\ud800a"];

samples.forEach(function(x) {
  assert.sameValue(x.localeCompare(x), 0, "reflexive for " + JSON.stringify(x));
  samples.forEach(function(y) {
    var xy = sign(x.localeCompare(y));
    var yx = sign(y.localeCompare(x));
    assert.sameValue(xy + yx, 0, "antisymmetric for " + JSON.stringify([x, y]));
    assert.sameValue(sign(collator.compare(x, y)), xy,
      "localeCompare agrees with Collator for " + JSON.stringify([x, y]));
    if (x !== y) {
      assert.notSameValue(xy, 0, "distinct strings " + JSON.stringify([x, y]));
    }
  });
});

assert.sameValue("\ud800".localeCompare("\ud800"), 0, "same lone surrogate");
assert.notSameValue("\ud800".localeCompare("\ud801"), 0, "different lone surrogates");
assert.notSameValue("\ud800".localeCompare("�"), 0, "lone surrogate against U+FFFD");
assert.sameValue("a\ud800".localeCompare("A\ud800", undefined, { sensitivity: "base" }), 0,
  "sensitivity still applies to the rest of the string");
assert.sameValue("\ud800".localeCompare("\ud801", undefined, { sensitivity: "base" }) < 0, true,
  "order follows the lone surrogates");
