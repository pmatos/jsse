/*---
description: >
  Intl.Locale is not a service constructor, so it has no supportedLocalesOf
  static method; only the service constructors define one.
features: [Intl.Locale]
---*/

// Spec: ECMA-402 "Properties of the Intl.Locale Constructor" lists only
// Intl.Locale.prototype. supportedLocalesOf is specified per service
// constructor (Collator, DateTimeFormat, DisplayNames, DurationFormat,
// ListFormat, NumberFormat, PluralRules, RelativeTimeFormat, Segmenter).

if (Intl.Locale.supportedLocalesOf !== undefined) {
  throw new Test262Error('Intl.Locale.supportedLocalesOf should be undefined, got ' +
    typeof Intl.Locale.supportedLocalesOf);
}

if (Object.prototype.hasOwnProperty.call(Intl.Locale, 'supportedLocalesOf')) {
  throw new Test262Error('Intl.Locale must not have an own supportedLocalesOf property');
}

var keys = Object.getOwnPropertyNames(Intl.Locale).sort();
var expectedKeys = ['length', 'name', 'prototype'];
if (keys.join() !== expectedKeys.join()) {
  throw new Test262Error('Intl.Locale own property names: expected ' +
    expectedKeys.join() + ', got ' + keys.join());
}

class SubLocale extends Intl.Locale {}
if (SubLocale.supportedLocalesOf !== undefined) {
  throw new Test262Error('a subclass of Intl.Locale must not inherit supportedLocalesOf');
}

var serviceConstructors = [
  'Collator', 'DateTimeFormat', 'DisplayNames', 'DurationFormat', 'ListFormat',
  'NumberFormat', 'PluralRules', 'RelativeTimeFormat', 'Segmenter'
];
for (var i = 0; i < serviceConstructors.length; i++) {
  var ctorName = serviceConstructors[i];
  if (typeof Intl[ctorName].supportedLocalesOf !== 'function') {
    throw new Test262Error('Intl.' + ctorName + '.supportedLocalesOf must remain a function');
  }
}
