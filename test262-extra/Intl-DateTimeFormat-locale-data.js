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

// dateStyle:"short" year width follows the locale's own short-date pattern
// (en-US uses two digits, fr-FR/ja-JP use the full year) for every year, not
// just years inside ICU's 2-digit window. Only an explicit year:"2-digit"
// forces the two-digit form everywhere.
var recentShort = Date.UTC(2020, 0, 2);
var historicalShort = Date.UTC(1886, 4, 1);

assertSame(
  new Intl.DateTimeFormat('fr-FR', { dateStyle: 'short', timeZone: 'UTC' })
    .format(recentShort),
  '02/01/2020',
  'French dateStyle:short keeps the locale full year (recent)');

assertSame(
  new Intl.DateTimeFormat('fr-FR', { dateStyle: 'short', timeZone: 'UTC' })
    .format(historicalShort),
  '01/05/1886',
  'French dateStyle:short keeps the locale full year (historical)');

assertSame(
  new Intl.DateTimeFormat('ja-JP', { dateStyle: 'short', timeZone: 'UTC' })
    .format(recentShort),
  '2020/01/02',
  'Japanese dateStyle:short keeps the locale full year');

assertSame(
  new Intl.DateTimeFormat('en-US', { dateStyle: 'short', timeZone: 'UTC' })
    .format(recentShort),
  '1/2/20',
  'English dateStyle:short uses a two-digit year (recent, in ICU window)');

assertSame(
  new Intl.DateTimeFormat('en-US', { dateStyle: 'short', timeZone: 'UTC' })
    .format(historicalShort),
  '5/1/86',
  'English dateStyle:short uses a two-digit year (historical, out of window)');

assertSame(
  new Intl.DateTimeFormat('fr-FR', {
    year: '2-digit',
    month: '2-digit',
    day: '2-digit',
    timeZone: 'UTC'
  }).format(recentShort),
  '02/01/20',
  'explicit year:2-digit still truncates to two digits');

// fractionalSecond must be split out of the "second" part using the locale's
// own decimal separator. Many locales (fr-FR, de-DE) use a comma, not a dot,
// and the "arab" numbering system uses the Arabic decimal separator.
function partValue(parts, type) {
  for (var i = 0; i < parts.length; i++) {
    if (parts[i].type === type) {
      return parts[i].value;
    }
  }
  return undefined;
}

function separatorAfterSecond(parts) {
  for (var i = 0; i < parts.length - 1; i++) {
    if (parts[i].type === 'second' && parts[i + 1].type === 'literal') {
      return parts[i + 1].value;
    }
  }
  return undefined;
}

var fracTimestamp = Date.UTC(2020, 0, 2, 3, 4, 5, 6);

function fractionalFormat(locale) {
  return new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    fractionalSecondDigits: 3,
    hourCycle: 'h23',
    timeZone: 'UTC'
  });
}

var frFractional = fractionalFormat('fr-FR');
assertSame(frFractional.format(fracTimestamp), '03:04:05,006',
  'French fractional second uses a comma decimal separator');
var frParts = frFractional.formatToParts(fracTimestamp);
assertSame(partValue(frParts, 'second'), '05',
  'French formatToParts second part excludes the fraction');
assertSame(partValue(frParts, 'fractionalSecond'), '006',
  'French formatToParts exposes a fractionalSecond part');
assertSame(separatorAfterSecond(frParts), ',',
  'French fractionalSecond literal is a comma');

var enFractional = fractionalFormat('en-US');
assertSame(enFractional.format(fracTimestamp), '03:04:05.006',
  'English fractional second keeps the dot decimal separator');
assertSame(separatorAfterSecond(enFractional.formatToParts(fracTimestamp)), '.',
  'English fractionalSecond literal is a dot');

var arabFractional = fractionalFormat('en-US-u-nu-arab');
assertSame(arabFractional.format(fracTimestamp), '٠٣:٠٤:٠٥٫٠٠٦',
  'Arabic-numbered fractional second uses the Arabic decimal separator');
assertSame(separatorAfterSecond(arabFractional.formatToParts(fracTimestamp)), '٫',
  'Arabic-numbered fractionalSecond literal is the Arabic decimal separator');
