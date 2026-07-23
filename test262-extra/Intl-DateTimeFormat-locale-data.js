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

function partValue(parts, type) {
  for (var i = 0; i < parts.length; i++) {
    if (parts[i].type === type) {
      return parts[i].value;
    }
  }
  return undefined;
}

// An hour selected as "numeric" follows the effective locale's matched
// DateTime Format Record. Spanish (Spain) uses the unpadded H pattern even
// when minute and second fields are also present.
var numericHourTimestamp = Date.UTC(2020, 0, 2, 3, 4, 5);
var esNumericHour = new Intl.DateTimeFormat('es-ES', {
  hour: 'numeric',
  minute: 'numeric',
  second: 'numeric',
  hourCycle: 'h23',
  timeZone: 'UTC'
});

assertSame(
  esNumericHour.format(numericHourTimestamp),
  '3:04:05',
  'Spanish numeric hour follows the unpadded H pattern');

assertSame(
  partValue(esNumericHour.formatToParts(numericHourTimestamp), 'hour'),
  '3',
  'Spanish formatToParts exposes an unpadded numeric hour');

assertSame(
  new Intl.DateTimeFormat('es-ES', {
    hour: 'numeric',
    hourCycle: 'h23',
    timeZone: 'UTC'
  }).format(numericHourTimestamp),
  '3',
  'standalone Spanish numeric hour follows the unpadded H pattern');

assertSame(
  new Intl.DateTimeFormat('es-ES', {
    hour: '2-digit',
    hourCycle: 'h23',
    timeZone: 'UTC'
  }).format(numericHourTimestamp),
  '03',
  'explicit Spanish 2-digit hour remains padded');

assertSame(
  new Intl.DateTimeFormat('es-ES-u-nu-arab', {
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
    hourCycle: 'h23',
    timeZone: 'UTC'
  }).format(numericHourTimestamp),
  '٣:٠٤:٠٥',
  'Spanish numeric hour removes the locale zero digit');

function numericHms(locale) {
  return new Intl.DateTimeFormat(locale, {
    hour: 'numeric',
    minute: 'numeric',
    second: 'numeric',
    hourCycle: 'h23',
    timeZone: 'UTC'
  }).format(numericHourTimestamp);
}

assertSame(numericHms('es'), '3:04:05',
  'language-only Spanish resolves to Spain data and stays unpadded');
assertSame(numericHms('es-u-hc-h23'), '3:04:05',
  'Spanish numeric hour with a Unicode hour-cycle extension stays unpadded');
assertSame(numericHms('ca-ES'), '3:04:05',
  'Catalan numeric hour remains unpadded');
assertSame(numericHms('es-MX'), '03:04:05',
  'Mexican Spanish numeric hour remains padded');
assertSame(numericHms('es-419'), '03:04:05',
  'Latin-American Spanish (es-419) numeric hour remains padded');
assertSame(numericHms('en-US'), '03:04:05',
  'English numeric hour remains padded');
assertSame(numericHms('fr-FR'), '03:04:05',
  'French numeric hour remains padded');
assertSame(numericHms('it-IT'), '03:04:05',
  'Italian numeric hour remains padded in an h:m:s pattern');

// A requested tag carrying an explicit script subtag that merely repeats the
// language's own default (e.g. "Latn" for Spanish) isn't literal locale-data
// key, so resolution strips it — and any following region — down to the bare
// language, same as Node/V8. This differs from the bare-region form (es-419
// stays padded) because the explicit script pushes past es-419 entirely.
assertSame(numericHms('es-Latn-ES'), '3:04:05',
  'es-Latn-ES resolves past the redundant script down to Spain Spanish data');
assertSame(numericHms('es-Latn'), '3:04:05',
  'es-Latn (script-only) resolves down to Spain Spanish data');
assertSame(numericHms('es-Latn-419'), '3:04:05',
  'es-Latn-419 resolves down to bare Spanish, unlike padded bare es-419');

assertSame(
  new Intl.DateTimeFormat('es-Latn-ES').resolvedOptions().locale, 'es',
  'es-Latn-ES resolvedOptions().locale reports the minimized bare language');
assertSame(
  new Intl.DateTimeFormat('es-Latn').resolvedOptions().locale, 'es',
  'es-Latn resolvedOptions().locale reports the minimized bare language');
assertSame(
  new Intl.DateTimeFormat('es-Latn-419').resolvedOptions().locale, 'es',
  'es-Latn-419 resolvedOptions().locale reports the minimized bare language');
assertSame(
  new Intl.DateTimeFormat('es-Latn-ES-u-hc-h23').resolvedOptions().locale,
  'es-u-hc-h23',
  'locale lookup preserves supported Unicode extensions on the matched locale');

// Genuinely multi-script languages (CLDR ships distinct per-script data, e.g.
// Simplified vs Traditional Chinese) must keep their explicit script subtag
// rather than being folded away like the Spanish cases above.
assertSame(
  new Intl.DateTimeFormat('zh-Hant-TW').resolvedOptions().locale, 'zh-Hant-TW',
  'zh-Hant-TW keeps its script-significant subtag untouched');
assertSame(
  new Intl.DateTimeFormat('sr-Cyrl-RS').resolvedOptions().locale, 'sr-Cyrl-RS',
  'sr-Cyrl-RS keeps its script-significant subtag untouched');

// timeStyle presets select the same unpadded-H record as hour:"numeric" for
// Spain-based Spanish (the hour option is undefined, so the correction must key
// off "not explicitly 2-digit" rather than requiring hour:"numeric"). Only an
// explicit hour:"2-digit" keeps the leading zero, and region-carrying
// Latin-American es-* stay padded.
function timeStyleHour(locale, style, timestamp) {
  return new Intl.DateTimeFormat(locale, {
    timeStyle: style,
    hourCycle: 'h23',
    timeZone: 'UTC'
  }).format(timestamp === undefined ? numericHourTimestamp : timestamp);
}

assertSame(timeStyleHour('es-ES', 'medium'), '3:04:05',
  'Spanish timeStyle:medium uses the unpadded H hour');
assertSame(timeStyleHour('es-ES', 'short'), '3:04',
  'Spanish timeStyle:short uses the unpadded H hour');
assertSame(timeStyleHour('es', 'medium'), '3:04:05',
  'language-only Spanish timeStyle:medium uses the unpadded H hour');
assertSame(timeStyleHour('es-ES', 'medium', Date.UTC(2020, 0, 2, 0, 4, 5)), '0:04:05',
  'Spanish timeStyle midnight hour is a single zero, not 00');
assertSame(timeStyleHour('es-ES', 'medium', Date.UTC(2020, 0, 2, 13, 4, 5)), '13:04:05',
  'Spanish timeStyle two-digit hours (13) are left untouched');
assertSame(timeStyleHour('es-MX', 'medium'), '03:04:05',
  'Mexican Spanish timeStyle hour remains padded');

// The correction reaches every formatter entry point, not just format().
var esTimeStyle = new Intl.DateTimeFormat('es-ES', {
  timeStyle: 'medium',
  hourCycle: 'h23',
  timeZone: 'UTC'
});
var laterHour = Date.UTC(2020, 0, 2, 5, 4, 5);
assertSame(partValue(esTimeStyle.formatToParts(numericHourTimestamp), 'hour'), '3',
  'Spanish timeStyle formatToParts exposes an unpadded hour');
assertSame(esTimeStyle.formatRange(numericHourTimestamp, laterHour),
  '3:04:05 – 5:04:05',
  'Spanish timeStyle formatRange un-pads both endpoint hours');

// An explicit hour:"2-digit" must still keep its leading zero even in an
// otherwise all-numeric time (guards the "!= 2-digit" broadening).
assertSame(
  new Intl.DateTimeFormat('es-ES', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
    timeZone: 'UTC'
  }).format(numericHourTimestamp),
  '03:04:05',
  'explicit Spanish 2-digit hour stays padded alongside numeric minute/second');

// fractionalSecond must be split out of the "second" part using the locale's
// own decimal separator. Many locales (fr-FR, de-DE) use a comma, not a dot,
// and the "arab" numbering system uses the Arabic decimal separator.
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
