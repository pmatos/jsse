// Intl.DateTimeFormat must preserve independently requested textual component
// widths while selecting the effective locale's pattern.
//
// Spec: ECMA-402 BasicFormatMatcher and FormatDateTimePattern preserve the
// requested weekday, era, and month representations in the selected format.

function assertSame(actual, expected, message) {
  if (actual !== expected) {
    throw new Test262Error(
      message + ': expected "' + expected + '", got "' + actual + '"');
  }
}

var timestamp = Date.UTC(1970, 0, 1);

function assertFormat(locale, options, expected, expectedFields, message) {
  options.timeZone = 'UTC';
  var formatter = new Intl.DateTimeFormat(locale, options);
  var formatted = formatter.format(timestamp);
  assertSame(formatted, expected, message);

  var parts = formatter.formatToParts(timestamp);
  var joined = '';
  for (var i = 0; i < parts.length; i++) {
    joined += parts[i].value;
  }
  assertSame(joined, formatted, message + ' formatToParts values reproduce format');

  for (var field in expectedFields) {
    var actual;
    for (var j = 0; j < parts.length; j++) {
      if (parts[j].type === field) {
        actual = parts[j].value;
        break;
      }
    }
    assertSame(actual, expectedFields[field], message + ' ' + field + ' part');
  }
}

assertFormat('en-US', {
  weekday: 'short',
  year: 'numeric',
  month: 'long',
  day: 'numeric'
}, 'Thu, January 1, 1970', {
  weekday: 'Thu', month: 'January'
}, 'English short weekday and long month');

assertFormat('en-US', {
  weekday: 'long',
  year: 'numeric',
  month: 'short',
  day: 'numeric'
}, 'Thursday, Jan 1, 1970', {
  weekday: 'Thursday', month: 'Jan'
}, 'English long weekday and short month');

assertFormat('en-US', {
  weekday: 'narrow',
  era: 'long',
  year: 'numeric',
  month: 'short',
  day: 'numeric'
}, 'T, Jan 1, 1970 Anno Domini', {
  weekday: 'T', month: 'Jan', era: 'Anno Domini'
}, 'English narrow weekday, short month, and long era');

assertFormat('fr-FR', {
  weekday: 'short',
  year: 'numeric',
  month: 'long',
  day: 'numeric'
}, 'jeu. 1 janvier 1970', {
  weekday: 'jeu.', month: 'janvier'
}, 'French short weekday and long month');

assertFormat('fr-FR', {
  weekday: 'long',
  era: 'short',
  year: 'numeric',
  month: 'short',
  day: 'numeric'
}, 'jeudi 1 janv. 1970 ap. J.-C.', {
  weekday: 'jeudi', month: 'janv.', era: 'ap. J.-C.'
}, 'French long weekday, short month, and short era');

assertFormat('ja-JP', {
  weekday: 'short',
  year: 'numeric',
  month: 'long',
  day: 'numeric'
}, '1970年1月1日(木)', {
  weekday: '木', month: '1'
}, 'Japanese short weekday and long month');

assertFormat('ja-JP', {
  weekday: 'long',
  year: 'numeric',
  month: 'short',
  day: 'numeric'
}, '1970年1月1日木曜日', {
  weekday: '木曜日', month: '1'
}, 'Japanese long weekday and short month');

assertFormat('de-DE', {
  weekday: 'narrow',
  era: 'long',
  year: 'numeric',
  month: 'short',
  day: 'numeric'
}, 'D, 1. Jan. 1970 n. Chr.', {
  weekday: 'D', month: 'Jan.', era: 'n. Chr.'
}, 'German narrow weekday, short month, and long era');

var enDateTime = new Intl.DateTimeFormat('en-US', {
  weekday: 'short',
  year: 'numeric',
  month: 'long',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
  second: '2-digit',
  fractionalSecondDigits: 3,
  timeZone: 'UTC'
});
var enDateTimeParts = enDateTime.formatToParts(
  Date.UTC(1970, 0, 1, 15, 4, 5, 123));
var enDateTimeFields = {};
for (var k = 0; k < enDateTimeParts.length; k++) {
  if (enDateTimeParts[k].type !== 'literal') {
    enDateTimeFields[enDateTimeParts[k].type] = enDateTimeParts[k].value;
  }
}
assertSame(enDateTimeFields.weekday, 'Thu',
  'fractional seconds do not widen the English weekday');
assertSame(enDateTimeFields.month, 'January',
  'fractional seconds do not narrow the English month');
assertSame(enDateTimeFields.fractionalSecond, '123',
  'mixed-width pattern retains fractional seconds');
