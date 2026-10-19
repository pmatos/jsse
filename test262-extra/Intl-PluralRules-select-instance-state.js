// Intl.PluralRules.prototype.select / selectRange depend only on the
// instance's immutable [[Locale]], [[Type]], [[Notation]] and
// [[CompactDisplay]] slots, so repeated calls, calls interleaved across
// instances, and calls made while ToNumber runs user code (a throwing or
// reentrant valueOf) must all answer exactly as a fresh instance would.
//
// Spec: ECMA-402 sec-intl.pluralrules.prototype.select (ToNumber(value), then
// ResolvePlural), sec-intl.pluralrules.prototype.selectrange,
// sec-resolveplural, sec-resolvepluralrange

var enCardinal = new Intl.PluralRules("en");
var enOrdinal = new Intl.PluralRules("en", { type: "ordinal" });
var arCardinal = new Intl.PluralRules("ar");
var frCompact = new Intl.PluralRules("fr", { notation: "compact" });

function check(actual, expected, message) {
  if (actual !== expected) {
    throw new Test262Error(message + ": expected " + expected + ", got " + actual);
  }
}

// Interleaved instances of different locale/type never share rule data.
for (var round = 0; round < 3; round++) {
  check(enCardinal.select(1), "one", "en cardinal 1");
  check(enCardinal.select(2), "other", "en cardinal 2");
  check(enOrdinal.select(1), "one", "en ordinal 1");
  check(enOrdinal.select(2), "two", "en ordinal 2");
  check(enOrdinal.select(3), "few", "en ordinal 3");
  check(enOrdinal.select(4), "other", "en ordinal 4");
  check(arCardinal.select(0), "zero", "ar 0");
  check(arCardinal.select(2), "two", "ar 2");
  check(arCardinal.select(11), "many", "ar 11");
  check(arCardinal.select(100), "other", "ar 100");
  check(frCompact.select(1300000), "many", "fr compact 1.3M");
  check(enCardinal.selectRange(1, 2), "other", "en selectRange(1, 2)");
  check(arCardinal.selectRange(1, 5), "few", "ar selectRange(1, 5)");
  check(arCardinal.selectRange(11, 11), "many", "ar selectRange(11, 11)");
}

// Non-finite and non-numeric inputs.
check(enCardinal.select(NaN), "other", "NaN");
check(enCardinal.select(undefined), "other", "undefined");
check(typeof enCardinal.select(Infinity), "string", "Infinity");
check(enCardinal.select(-Infinity), enCardinal.select(Infinity), "-Infinity");
check(enCardinal.select("1"), "one", "string \"1\"");
check(enCardinal.select(-1), enCardinal.select(1), "negative");
check(enCardinal.select(1.5), "other", "1.5");

assert.throws(RangeError, function() { enCardinal.selectRange(NaN, 1); }, "start NaN");
assert.throws(RangeError, function() { enCardinal.selectRange(1, NaN); }, "end NaN");
assert.throws(TypeError, function() { enCardinal.selectRange(undefined, 1); }, "start undefined");
assert.throws(TypeError, function() { enCardinal.selectRange(1, undefined); }, "end undefined");

// A throwing coercion propagates and leaves the instance usable.
var boom = new Test262Error("valueOf");
assert.throws(Test262Error, function() {
  enCardinal.select({ valueOf: function() { throw boom; } });
}, "select propagates ToNumber abrupt completion");
assert.throws(Test262Error, function() {
  enCardinal.selectRange({ valueOf: function() { throw boom; } }, 1);
}, "selectRange propagates ToNumber abrupt completion");
check(enCardinal.select(1), "one", "after a throwing valueOf");

// Reentrancy: valueOf calls back into the same instance, in both directions.
var order = [];
var reentrant = {
  valueOf: function() {
    order.push("valueOf");
    order.push(enCardinal.select(2));
    order.push(enCardinal.selectRange(1, 2));
    return 1;
  }
};
check(enCardinal.select(reentrant), "one", "reentrant select result");
check(order.join(), "valueOf,other,other", "reentrant calls observed");

var rangeOrder = [];
var tracker = function(label, value) {
  return { valueOf: function() { rangeOrder.push(label); return value; } };
};
check(arCardinal.selectRange(tracker("start", 1), tracker("end", 5)), "few", "ar range via objects");
check(rangeOrder.join(), "start,end", "selectRange coerces start before end");

// resolvedOptions is stable across select calls.
var before = JSON.stringify(arCardinal.resolvedOptions());
arCardinal.select(5);
arCardinal.selectRange(1, 5);
check(JSON.stringify(arCardinal.resolvedOptions()), before, "resolvedOptions after select");
check(arCardinal.resolvedOptions().pluralCategories.join(), "zero,one,two,few,many,other", "ar categories");
check(enOrdinal.resolvedOptions().pluralCategories.join(), "one,two,few,other", "en ordinal categories");

// Returned categories are ordinary string primitives.
var cat = enCardinal.select(1);
check(typeof cat, "string", "typeof category");
check(cat + "!", "one!", "category concatenation");
check(Object.is(cat, enCardinal.select(1)), true, "category identity as primitive");

// Calling on a foreign receiver still throws.
assert.throws(TypeError, function() { Intl.PluralRules.prototype.select.call({}, 1); }, "select on {}");
assert.throws(TypeError, function() { Intl.PluralRules.prototype.selectRange.call(new Intl.NumberFormat(), 1, 2); }, "selectRange on NumberFormat");
