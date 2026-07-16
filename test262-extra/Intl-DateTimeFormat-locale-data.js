// Intl.DateTimeFormat output must use the effective locale's symbols and
// patterns instead of English fallback tables.
//
// Spec: ECMA-402 PartitionDateTimePattern and FormatDateTimePattern create
// parts according to the effective locale and the selected format record.

function assertSame(actual, expected, message) {
  if (actual !== expected) {
    throw new Test262Error(
      message + ': expected "' + expected + '", got "' + actual + '"');
  }
}

assertSame(
  new Intl.DateTimeFormat('fr', {
    month: 'long',
    timeZone: 'UTC'
  }).format(0),
  'janvier',
  'French long month name');

assertSame(
  new Intl.DateTimeFormat('fr', {
    weekday: 'long',
    timeZone: 'UTC'
  }).format(0),
  'jeudi',
  'French long weekday name');

assertSame(
  new Intl.DateTimeFormat('ru', {
    month: 'long',
    timeZone: 'UTC'
  }).format(Date.UTC(2020, 2, 1)),
  'март',
  'Russian standalone long month name');

assertSame(
  new Intl.DateTimeFormat('my', {
    hour: 'numeric',
    hour12: true,
    timeZone: 'UTC'
  }).format(0),
  'နံနက် ၁၂',
  'Burmese day period, digits, and field order');

assertSame(
  new Intl.DateTimeFormat('fr', {
    year: 'numeric',
    era: 'short',
    timeZone: 'UTC'
  }).format(Date.parse('0000-01-01T00:00:00Z')),
  '1 av. J.-C.',
  'French era name');

var dateTime = new Intl.DateTimeFormat('en-US', {
  year: 'numeric',
  month: 'long',
  day: 'numeric',
  hour: 'numeric',
  minute: 'numeric',
  timeZoneName: 'short',
  timeZone: 'UTC'
});
var timestamp = Date.UTC(1982, 4, 25, 9, 23);
var formatted = dateTime.format(timestamp);
if (formatted.indexOf(' at ') < 0) {
  throw new Test262Error(
    'long English date/time pattern must use locale glue " at ", got "' +
    formatted + '"');
}

var parts = dateTime.formatToParts(timestamp);
var joined = '';
var foundGlue = false;
for (var i = 0; i < parts.length; i++) {
  joined += parts[i].value;
  if (parts[i].type === 'literal' && parts[i].value === ' at ') {
    foundGlue = true;
  }
}
assertSame(joined, formatted, 'formatToParts values must reproduce format');
if (!foundGlue) {
  throw new Test262Error(
    'formatToParts must expose locale date/time glue as a literal');
}

assertSame(
  new Intl.DateTimeFormat('fr', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    hour: 'numeric',
    minute: 'numeric',
    timeZoneName: 'short',
    timeZone: 'UTC'
  }).format(timestamp),
  '25 mai 1982 à 09:23 UTC',
  'French date/time pattern and glue');
