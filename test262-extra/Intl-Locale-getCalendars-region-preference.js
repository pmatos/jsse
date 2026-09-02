// Intl.Locale.prototype.getCalendars must use the locale's region preference
// when no explicit calendar keyword is present.
//
// Spec: ECMA-402 CalendarsOfLocale and RegionPreference.

function assertCalendars(locale, expected) {
  var actual = new Intl.Locale(locale).getCalendars();

  if (actual.length !== expected.length) {
    throw new Test262Error(
      locale + ': expected ' + expected.length + ' calendars, got ' + actual.length);
  }

  for (var i = 0; i < expected.length; i++) {
    if (actual[i] !== expected[i]) {
      throw new Test262Error(
        locale + ': expected calendar ' + expected[i] + ' at index ' + i +
        ', got ' + actual[i]);
    }
  }
}

assertCalendars('th-TH', ['buddhist', 'gregory']);
assertCalendars('en-US', ['gregory']);
assertCalendars('und-001', ['gregory']);
assertCalendars('th-TH-u-ca-japanese', ['japanese']);

var egyptianCalendars = [
  'gregory',
  'coptic',
  'islamic',
  'islamic-civil',
  'islamic-tbla'
];
assertCalendars('ar-EG', egyptianCalendars);
assertCalendars('en-EG', egyptianCalendars);
