// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
description: >
  A String exotic object exposes an own character property for a key P only when
  P is the CanonicalNumericIndexString of an integer index in [0, length). A key
  that merely parses as a number under a host integer parser -- a leading "+", a
  leading zero ("01"), a non-integral or non-finite spelling -- is NOT such an
  index and must behave as an ordinary (absent) property across every MOP
  operation: [[Get]], [[HasProperty]], [[GetOwnProperty]], [[DefineOwnProperty]],
  [[Delete]], and [[OwnPropertyKeys]]. Indexing is by UTF-16 code unit, so both
  halves of a supplementary code point are in range. Expected values are
  cross-checked with Node.
info: |
  10.4.3.5 StringGetOwnProperty ( S, P )
    2. Let index be CanonicalNumericIndexString(P).
    3. If index is undefined, return undefined.
    4. If IsIntegralNumber(index) is false, return undefined.
    5. If index is -0𝔽, return undefined.
    ...
    8. If ℝ(index) < 0 or len ≤ ℝ(index), return undefined.

  7.1.21 CanonicalNumericIndexString ( argument )
    ... 3. If SameValue(! ToString(n), argument) is false, return undefined.
includes: [propertyHelper.js]
---*/

var NON_INDEX_KEYS = ["01", "00", "+1", "-1", "1.0", "1.5", "1e0", " 1", "0x1", "Infinity", "NaN"];

// --- Primitive string [[Get]] and wrapper [[Get]] ---
NON_INDEX_KEYS.forEach(function (k) {
  assert.sameValue("abc"[k], undefined, 'primitive read "abc"[' + JSON.stringify(k) + ']');
  assert.sameValue(Object("abc")[k], undefined, 'wrapper read Object("abc")[' + JSON.stringify(k) + ']');
});
assert.sameValue("abc"["0"], "a", 'canonical index "0"');
assert.sameValue("abc"["1"], "b", 'canonical index "1"');
assert.sameValue("abc"["2"], "c", 'canonical index "2"');
assert.sameValue("abc"["3"], undefined, 'out-of-range index "3"');

// --- [[HasProperty]] ---
var wrapper = Object("abc");
NON_INDEX_KEYS.forEach(function (k) {
  assert.sameValue(k in wrapper, false, '"' + k + '" in Object("abc")');
});
assert.sameValue("1" in wrapper, true, '"1" in Object("abc")');
assert.sameValue("3" in wrapper, false, '"3" in Object("abc")');

// --- [[GetOwnProperty]] ---
NON_INDEX_KEYS.forEach(function (k) {
  assert.sameValue(
    Object.getOwnPropertyDescriptor(wrapper, k),
    undefined,
    "getOwnPropertyDescriptor of look-alike " + JSON.stringify(k)
  );
});
// Canonical index property has the spec-mandated attributes.
verifyProperty(Object("abc"), "1", {
  value: "b",
  writable: false,
  enumerable: true,
  configurable: false,
});

// --- [[OwnPropertyKeys]] ---
assert.sameValue(Object.keys(Object("abc")).join(","), "0,1,2", "Object.keys");
assert.sameValue(
  Object.getOwnPropertyNames(Object("abc")).join(","),
  "0,1,2,length",
  "getOwnPropertyNames"
);

// --- [[DefineOwnProperty]] on a look-alike key succeeds (ordinary property) ---
var def = Object("abc");
Object.defineProperty(def, "01", { value: "z", writable: true, enumerable: true, configurable: true });
assert.sameValue(def["01"], "z", 'defineProperty(o, "01", ...) creates an ordinary property');
var d = Object.getOwnPropertyDescriptor(def, "01");
assert.sameValue(d.configurable, true, "the defined property is configurable, unlike a real index");

// --- [[Set]] on a look-alike key stores an ordinary property ---
// The non-writable string-index guard must not fire for a key that is not a
// CanonicalNumericIndexString, so an ordinary assignment succeeds.
var setObj = Object("abc");
setObj["01"] = "z";
assert.sameValue(setObj["01"], "z", 'assigning to "01" creates an ordinary property');

// --- [[Delete]] via Reflect (mode-independent: never throws) ---
assert.sameValue(Reflect.deleteProperty(Object("abc"), "1"), false, "own index 1 is non-configurable");
assert.sameValue(Reflect.deleteProperty(Object("abc"), "01"), true, '"01" is an ordinary absent property');
assert.sameValue(Reflect.deleteProperty(Object("abc"), "3"), true, "out-of-range index is absent");

// --- UTF-16 code-unit indexing (supplementary code point occupies two indices) ---
var poo = Object("\u{1F4A9}"); // U+1F4A9, encoded as the surrogate pair D83D DCA9
assert.sameValue(poo.length, 2, "length counts UTF-16 code units");
assert.sameValue("0" in poo, true, "lead surrogate is own index 0");
assert.sameValue("1" in poo, true, "trail surrogate is own index 1");
assert.sameValue("2" in poo, false, "index 2 is out of range");
// Both halves are non-configurable; delete must report false (this is the
// code-unit-vs-code-point regression: a code-point count would make index 1 absent).
assert.sameValue(Reflect.deleteProperty(Object("\u{1F4A9}"), "1"), false, "trail surrogate index is in range");
assert.sameValue(Reflect.deleteProperty(Object("\u{1F4A9}"), "0"), false, "lead surrogate index is in range");
