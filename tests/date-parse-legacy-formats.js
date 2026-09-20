// Date.parse (§21.4.3.2) may fall back to implementation-specific formats for
// strings outside the Date Time String Format. jsse accepts the legacy
// `M/D/YYYY` numeric form (interpreted as local midnight), matching V8 and
// SpiderMonkey, and returns NaN for anything unrecognizable or out of bounds.

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

// A first component that cannot be a month selects year-first Y/M/D.
sameValue(Date.parse("2011/08/04"), local(2011, 7, 4), "2011/08/04");
sameValue(Date.parse("2011/8/4"), local(2011, 7, 4), "2011/8/4");
sameValue(Date.parse("50/1/1"), local(1950, 0, 1), "50/1/1");
sameValue(Date.parse("32/1/1"), local(2032, 0, 1), "32/1/1");

var invalidShort = ["13/1/1", "31/1/1", "99/1/99", "0/10/0"];
for (var j = 0; j < invalidShort.length; j++) {
  sameValue(Date.parse(invalidShort[j]), NaN, JSON.stringify(invalidShort[j]));
}
