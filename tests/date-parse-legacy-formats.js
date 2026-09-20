// Date.parse (§21.4.3.2) may fall back to implementation-specific formats for
// strings outside the Date Time String Format. jsse accepts date-only legacy
// forms (numeric `M/D/YYYY` and `Y/M/D`, two-digit years, and written months
// such as `May 1, 2000`), interpreted as local midnight, and returns NaN for
// anything unrecognizable or out of bounds. Like V8 and SpiderMonkey it does
// not validate the day against the month length (`2/30/2000` rolls over), but
// unlike V8 it takes zero-padded 3+ digit years literally.

function sameValue(actual, expected, label) {
  if (!Object.is(actual, expected)) {
    throw new Error(label + ": expected " + String(expected) +
      ", got " + String(actual));
  }
}

function local(y, m, d) {
  return new Date(y, m, d).getTime();
}

sameValue(Date.parse("08/04/2011"), local(2011, 7, 4), "08/04/2011");
sameValue(Date.parse("8/4/2011"), local(2011, 7, 4), "8/4/2011");
sameValue(Date.parse("2/29/2000"), local(2000, 1, 29), "2/29/2000");
sameValue(Date.parse("12/31/1999"), local(1999, 11, 31), "12/31/1999");
sameValue(Date.parse(" 08/04/2011 "), local(2011, 7, 4), "surrounding whitespace");
sameValue(new Date("08/04/2011").getTime(), local(2011, 7, 4), "Date constructor");

sameValue(Date.parse("2010-07-02") < Date.parse("08/04/2011"), true, "ordering");

var invalid = [
  "1/32/2000", "0/1/2000", "1/0/2000", "13/13/13",
  "1/1/", "1//2000", "/1/2000", "1/1/2000/1", "1/1/-5", "1/1/+2000",
];
for (var i = 0; i < invalid.length; i++) {
  sameValue(Date.parse(invalid[i]), NaN, JSON.stringify(invalid[i]));
}

// Two-digit years: < 50 maps to 20xx, >= 50 to 19xx; 3+ digits are literal.
sameValue(Date.parse("1/1/0"), local(2000, 0, 1), "1/1/0");
sameValue(Date.parse("1/1/49"), local(2049, 0, 1), "1/1/49");
sameValue(Date.parse("1/1/50"), local(1950, 0, 1), "1/1/50");
sameValue(Date.parse("1/1/99"), local(1999, 0, 1), "1/1/99");
sameValue(Date.parse("12/1/1"), local(2001, 11, 1), "12/1/1");

var literalYear = new Date(2000, 0, 1);
literalYear.setFullYear(100);
sameValue(Date.parse("1/1/100"), literalYear.getTime(), "1/1/100");
literalYear.setFullYear(999);
sameValue(Date.parse("1/1/999"), literalYear.getTime(), "1/1/999");
literalYear.setFullYear(99);
sameValue(Date.parse("1/1/0099"), literalYear.getTime(), "1/1/0099 is a literal year");

sameValue(Date.parse("2/30/2000"), local(2000, 2, 1), "2/30/2000 rolls over");

// A first component that cannot be a month selects year-first Y/M/D.
sameValue(Date.parse("2011/08/04"), local(2011, 7, 4), "2011/08/04");
sameValue(Date.parse("2011/8/4"), local(2011, 7, 4), "2011/8/4");
sameValue(Date.parse("50/1/1"), local(1950, 0, 1), "50/1/1");
sameValue(Date.parse("32/1/1"), local(2032, 0, 1), "32/1/1");

var invalidShort = ["13/1/1", "31/1/1", "99/1/99", "0/10/0"];
for (var j = 0; j < invalidShort.length; j++) {
  sameValue(Date.parse(invalidShort[j]), NaN, JSON.stringify(invalidShort[j]));
}

// Written months: one month name, two numbers, optional weekday name.
var may2000 = local(2000, 4, 1);
var writtenMay2000 = [
  "may 1 2000", "1 may 2000", "1 2000 may", "may 2000 1", "2000 may 1",
  "2000 1 may", "May 1, 2000", "Mon, May 1 2000", "MAY 1 2000",
  "Monday May 1 2000", "1 May, 2000",
];
for (var k = 0; k < writtenMay2000.length; k++) {
  sameValue(Date.parse(writtenMay2000[k]), may2000, JSON.stringify(writtenMay2000[k]));
}
sameValue(Date.parse("September 3 2001"), local(2001, 8, 3), "September 3 2001");
sameValue(Date.parse("dec 31 1999"), local(1999, 11, 31), "dec 31 1999");
sameValue(Date.parse("may 1 5"), local(2005, 4, 1), "may 1 5");
sameValue(Date.parse("may 1 0"), local(2000, 4, 1), "may 1 0");
literalYear.setFullYear(100);
literalYear.setMonth(4);
sameValue(Date.parse("may 1 100"), literalYear.getTime(), "may 1 100");

var invalidWritten = [
  "may 1999 1999", "may 0 0", "may 32 2000", "invalid date", "foo", "may",
  "may 1", "Mon May", "5/1 may 2000", "may may 1 2000", "Mon Tue may 1 2000",
  "may 1 2000 2001", "may 1st 2000", "may-1-2000",
];
for (var n = 0; n < invalidWritten.length; n++) {
  sameValue(Date.parse(invalidWritten[n]), NaN, JSON.stringify(invalidWritten[n]));
}
