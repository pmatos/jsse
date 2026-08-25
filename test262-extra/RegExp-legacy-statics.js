// Tests the value behavior of legacy RegExp static properties after exec/test.
// Spec: ECMAScript 2024, Annex B.1.2 — Legacy RegExp Features
//
// test262 only covers property descriptors and `this`-value validation for
// the legacy accessors. This test fills the value-behavior gap: after a
// successful match, the legacy statics must reflect the input, captures,
// and context strings.

// --- Setup: a match that exercises all legacy statics ---
var re = /(\d+)-(\w+)/;
var m = re.exec("abc 42-foo def");

// [[RegExpInput]] — the full input string of the last match
if (RegExp.input !== "abc 42-foo def") {
  throw new Test262Error('RegExp.input should be "abc 42-foo def", got: ' + RegExp.input);
}
if (RegExp["$_"] !== "abc 42-foo def") {
  throw new Test262Error('RegExp.$_ should be "abc 42-foo def", got: ' + RegExp["$_"]);
}

// [[RegExpLastMatch]] — the full match text
if (RegExp.lastMatch !== "42-foo") {
  throw new Test262Error('RegExp.lastMatch should be "42-foo", got: ' + RegExp.lastMatch);
}
if (RegExp["$&"] !== "42-foo") {
  throw new Test262Error('RegExp.$& should be "42-foo", got: ' + RegExp["$&"]);
}

// [[RegExpLastParen]] — the last capture group's text
if (RegExp.lastParen !== "foo") {
  throw new Test262Error('RegExp.lastParen should be "foo", got: ' + RegExp.lastParen);
}
if (RegExp["$+"] !== "foo") {
  throw new Test262Error('RegExp.$+ should be "foo", got: ' + RegExp["$+"]);
}

// [[RegExpLeftContext]] — substring before the match
if (RegExp.leftContext !== "abc ") {
  throw new Test262Error('RegExp.leftContext should be "abc ", got: ' + RegExp.leftContext);
}
if (RegExp["$`"] !== "abc ") {
  throw new Test262Error('RegExp.$` should be "abc ", got: ' + RegExp["$`"]);
}

// [[RegExpRightContext]] — substring after the match
if (RegExp.rightContext !== " def") {
  throw new Test262Error('RegExp.rightContext should be " def", got: ' + RegExp.rightContext);
}
if (RegExp["$'"] !== " def") {
  throw new Test262Error("RegExp.$' should be ' def', got: " + RegExp["$'"]);
}

// [[RegExpN]] — $1..$9 capture groups
if (RegExp["$1"] !== "42") {
  throw new Test262Error('RegExp.$1 should be "42", got: ' + RegExp["$1"]);
}
if (RegExp["$2"] !== "foo") {
  throw new Test262Error('RegExp.$2 should be "foo", got: ' + RegExp["$2"]);
}
// $3..$9 should be empty (no matching groups)
if (RegExp["$3"] !== "") {
  throw new Test262Error('RegExp.$3 should be "", got: ' + RegExp["$3"]);
}

// --- Setter: RegExp.input is settable ---
RegExp.input = "overridden";
if (RegExp.input !== "overridden") {
  throw new Test262Error('RegExp.input after set should be "overridden", got: ' + RegExp.input);
}
// $_ is the same accessor as input
if (RegExp["$_"] !== "overridden") {
  throw new Test262Error('RegExp.$_ after set should be "overridden", got: ' + RegExp["$_"]);
}
// Setting [[RegExpInput]] must not alter contexts retained from the last match.
if (RegExp.leftContext !== "abc " || RegExp.rightContext !== " def") {
  throw new Test262Error("setting RegExp.input changed the last match contexts");
}

// Retained input and lazily materialized contexts preserve raw UTF-16 code
// units, including unpaired surrogates.
var surrogateInput = "\ud800A\udc00";
/A/.exec(surrogateInput);
if (
  RegExp.input.length !== 3 ||
  RegExp.input.charCodeAt(0) !== 0xd800 ||
  RegExp.input.charCodeAt(2) !== 0xdc00
) {
  throw new Test262Error("RegExp.input did not preserve lone surrogates");
}
if (
  RegExp.leftContext.length !== 1 ||
  RegExp.leftContext.charCodeAt(0) !== 0xd800
) {
  throw new Test262Error("RegExp.leftContext did not preserve the lead surrogate");
}
if (
  RegExp.rightContext.length !== 1 ||
  RegExp.rightContext.charCodeAt(0) !== 0xdc00
) {
  throw new Test262Error("RegExp.rightContext did not preserve the trail surrogate");
}

// Unicode match offsets must be translated back through the original UTF-16
// subject. A genuine scalar in JSSE's lone-surrogate PUA range must count as
// its two original code units rather than as a one-unit surrogate sentinel.
var supplementaryPua = String.fromCharCode(0xdb80, 0xdc00);
var puaContextInput = supplementaryPua + "x" + supplementaryPua;

function assertSupplementaryPuaContexts(re, label) {
  var puaMatch = re.exec(puaContextInput);
  if (puaMatch.index !== 2) {
    throw new Test262Error(label + " match index should be 2, got " + puaMatch.index);
  }
  if (
    RegExp.leftContext.length !== 2 ||
    RegExp.leftContext.charCodeAt(0) !== 0xdb80 ||
    RegExp.leftContext.charCodeAt(1) !== 0xdc00
  ) {
    throw new Test262Error(label + " leftContext split a supplementary PUA scalar");
  }
  if (
    RegExp.rightContext.length !== 2 ||
    RegExp.rightContext.charCodeAt(0) !== 0xdb80 ||
    RegExp.rightContext.charCodeAt(1) !== 0xdc00
  ) {
    throw new Test262Error(label + " rightContext split a supplementary PUA scalar");
  }
}

assertSupplementaryPuaContexts(/x/, "non-Unicode");
assertSupplementaryPuaContexts(/x/u, "Unicode");

// --- Subclass receiver throws TypeError ---
class MyRegExp extends RegExp {}
var threw = false;
try {
  MyRegExp.lastMatch;
} catch (e) {
  if (!(e instanceof TypeError)) {
    throw new Test262Error("Expected TypeError for subclass receiver, got: " + e);
  }
  threw = true;
}
if (!threw) {
  throw new Test262Error("MyRegExp.lastMatch should throw TypeError");
}

// --- Non-RegExp-constructor receiver throws TypeError ---
threw = false;
try {
  Object.getOwnPropertyDescriptor(RegExp, "lastMatch").get.call({});
} catch (e) {
  if (!(e instanceof TypeError)) {
    throw new Test262Error("Expected TypeError for non-RegExp this, got: " + e);
  }
  threw = true;
}
if (!threw) {
  throw new Test262Error("Object.getOwnPropertyDescriptor(RegExp, 'lastMatch').get.call({}) should throw TypeError");
}

// --- After a new match, values update ---
var re2 = /(\w+)@(\w+)\.(\w+)/;
re2.exec("user@example.com");
if (RegExp["$1"] !== "user") {
  throw new Test262Error('RegExp.$1 after new match should be "user", got: ' + RegExp["$1"]);
}
if (RegExp["$2"] !== "example") {
  throw new Test262Error('RegExp.$2 after new match should be "example", got: ' + RegExp["$2"]);
}
if (RegExp["$3"] !== "com") {
  throw new Test262Error('RegExp.$3 after new match should be "com", got: ' + RegExp["$3"]);
}
if (RegExp.lastMatch !== "user@example.com") {
  throw new Test262Error('RegExp.lastMatch after new match should be "user@example.com", got: ' + RegExp.lastMatch);
}
