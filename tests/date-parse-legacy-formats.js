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
