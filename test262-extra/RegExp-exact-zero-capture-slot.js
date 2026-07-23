// Capturing groups occupy a result slot even when an exact-zero quantifier
// prevents them from participating in a match.
// Spec: ECMAScript 2026, sec-regexpinitialize and sec-regexpbuiltinexec.

var single = /(a){0}/.exec("");
assert.sameValue(single.length, 2, "trailing {0} capture keeps its slot");
assert.sameValue(single[0], "", "full match for trailing {0} capture");
assert.sameValue(single[1], undefined, "trailing {0} capture is undefined");

var afterCapture = /(b)(a){0,0}/.exec("b");
assert.sameValue(afterCapture.length, 3, "trailing {0,0} capture keeps its slot");
assert.sameValue(afterCapture[0], "b", "full match before trailing {0,0} capture");
assert.sameValue(afterCapture[1], "b", "participating capture before trailing {0,0}");
assert.sameValue(afterCapture[2], undefined, "trailing {0,0} capture is undefined");

var withIndices = /(a){0}/d.exec("");
assert.sameValue(withIndices.indices.length, 2, "indices keeps the trailing capture slot");
assert.sameValue(withIndices.indices[1], undefined, "unmatched trailing capture has no indices");

assert.sameValue(
  "".replace(/(a){0}/, "x$1y"),
  "xy",
  "replacement recognizes the unmatched capture"
);

var named = /(?<last>a){0}/d.exec("");
assert.sameValue(named.length, 2, "trailing named capture keeps its slot");
assert.sameValue(typeof named.groups, "object", "named captures create a groups object");
assert.sameValue(
  Object.prototype.hasOwnProperty.call(named.groups, "last"),
  true,
  "groups has the trailing capture name"
);
assert.sameValue(named.groups.last, undefined, "trailing named capture is undefined");
assert.sameValue(
  named.indices.groups.last,
  undefined,
  "trailing named capture has no named indices"
);
