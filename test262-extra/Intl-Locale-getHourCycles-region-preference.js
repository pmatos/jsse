// Intl.Locale.prototype.getHourCycles must use the locale's region
// preference when no explicit hc keyword is present, and a valid but
// untabulated -u-rg- override must fall back to the CLDR world default
// rather than abandoning the override for the locale's ordinary region.
//
// Spec: ECMA-402 HourCyclesOfLocale and RegionPreference.

function assertHourCycles(locale, expected) {
  var actual = new Intl.Locale(locale).getHourCycles();

  if (actual.length !== expected.length) {
    throw new Test262Error(
      locale + ': expected ' + expected.length + ' hour cycles, got ' + actual.length);
  }

  for (var i = 0; i < expected.length; i++) {
    if (actual[i] !== expected[i]) {
      throw new Test262Error(
        locale + ': expected hour cycle ' + expected[i] + ' at index ' + i +
        ', got ' + actual[i]);
    }
  }
}

assertHourCycles('en-US', ['h12']);
assertHourCycles('en-GB', ['h23']);
assertHourCycles('und-001', ['h23']);

// A real region override changes the result.
assertHourCycles('en-US-u-rg-gbzzzz', ['h23']);

// Region "150" (UN M49 "Europe") is a valid rg override with real CLDR
// data, but has no literal entry in the hour-cycle preference table. The
// lookup must fall back to the CLDR world default ("001" -> h23) rather
// than abandoning the override and using the locale's ordinary region
// (US -> h12).
assertHourCycles('en-US-u-rg-150zzzz', ['h23']);
