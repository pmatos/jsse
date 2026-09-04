/*---
description: >
  Date.prototype.toLocaleString, toLocaleDateString, and toLocaleTimeString run
  the thisTimeValue brand check on their receiver BEFORE consulting the
  `locales`/`options` arguments. A receiver without a [[DateValue]] internal
  slot must throw a TypeError from step 1 even when the `options` argument would
  itself throw while ToDateTimeOptions reads a property from it, and an Invalid
  Date must short-circuit to "Invalid Date" at step 2 without ever reading the
  options. This pins the ordering guaranteed by the shared Date brand-check
  seam. test262's own intl402 this-value-non-date coverage calls these methods
  with no arguments, so it never exercises this ordering.
esid: sec-date.prototype.tolocalestring
info: |
  Date.prototype.toLocaleString ( [ reserved1 [ , reserved2 ] ] )
    1. Let x be ? thisTimeValue(this value).
    2. If x is NaN, return "Invalid Date".
    3. Let options be ? ToDateTimeOptions(options, "any", "all").
    4. Let dateFormat be ? Construct(%DateTimeFormat%, ( locales, options )).
    5. Return ? FormatDateTime(dateFormat, x).

  Date.prototype.toLocaleDateString ( [ reserved1 [ , reserved2 ] ] )
    1. Let x be ? thisTimeValue(this value).
    2. If x is NaN, return "Invalid Date".
    3. Let options be ? ToDateTimeOptions(options, "date", "date").
    ...

  Date.prototype.toLocaleTimeString ( [ reserved1 [ , reserved2 ] ] )
    1. Let x be ? thisTimeValue(this value).
    2. If x is NaN, return "Invalid Date".
    3. Let options be ? ToDateTimeOptions(options, "time", "time").
    ...

  thisTimeValue ( value )
    1. If value is an Object and value has a [[DateValue]] internal slot, then
       a. Return value.[[DateValue]].
    2. Throw a TypeError exception.
---*/

var localeMethods = ["toLocaleString", "toLocaleDateString", "toLocaleTimeString"];

// An options argument that throws the instant ToDateTimeOptions reads any
// property from it. If the brand check (or the Invalid Date short-circuit) runs
// first, as required, none of these getters is ever touched.
function poisonedOptions(tag) {
  return {
    get localeMatcher() { throw new Test262Error("options read: " + tag); },
    get year() { throw new Test262Error("options read: " + tag); },
    get month() { throw new Test262Error("options read: " + tag); },
    get hour() { throw new Test262Error("options read: " + tag); },
    get dateStyle() { throw new Test262Error("options read: " + tag); },
    get timeStyle() { throw new Test262Error("options read: " + tag); }
  };
}

var nonDateReceivers = [
  ["ordinary object", {}],
  ["number", 5],
  ["string", "1970"],
  ["null", null],
  ["undefined", undefined],
  ["Date.prototype itself", Date.prototype],
  ["object inheriting from Date.prototype", Object.create(Date.prototype)]
];

localeMethods.forEach(function (name) {
  var method = Date.prototype[name];
  assert.sameValue(typeof method, "function", "Date.prototype." + name + " is callable");

  nonDateReceivers.forEach(function (entry) {
    var label = entry[0];
    var receiver = entry[1];
    assert.throws(
      TypeError,
      function () { method.call(receiver, undefined, poisonedOptions(name)); },
      "Date.prototype." + name + " brand-checks " + label + " before reading the options argument"
    );
  });
});

// An Invalid Date resolves the brand check but short-circuits to "Invalid Date"
// at step 2, still before any option processing: the poisoned options argument
// must not be consulted.
localeMethods.forEach(function (name) {
  var method = Date.prototype[name];
  var out = method.call(new Date(NaN), undefined, poisonedOptions(name));
  assert.sameValue(
    out,
    "Invalid Date",
    "Date.prototype." + name + " on an Invalid Date returns 'Invalid Date' without reading options"
  );
});

// Positive control: a real Date passes the brand check and formats without
// throwing, so the assertions above are not vacuously satisfied by every call
// throwing regardless of receiver.
localeMethods.forEach(function (name) {
  var method = Date.prototype[name];
  var out = method.call(new Date(0));
  assert.sameValue(
    typeof out,
    "string",
    "Date.prototype." + name + " on a real Date returns a formatted string"
  );
});
