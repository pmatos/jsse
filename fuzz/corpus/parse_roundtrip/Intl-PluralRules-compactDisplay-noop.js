// Intl.PluralRules with notation:"compact" surfaces the compactDisplay option
// via resolvedOptions (issue #171), but compactDisplay ("short" vs "long") must
// NOT change which plural category select/selectRange returns: under CLDR the
// plural operands (including the compact exponent `c`) are identical for short
// and long compact forms ("1K" and "1 thousand" have the same operands), so the
// category is the same. This is the documented no-op established by issue #174:
// [[CompactDisplay]] is threaded into the selection path to mirror ECMA-402
// PluralRuleSelect, but does not alter the result with the current locale data.
//
// Spec: ECMA-402 Intl.PluralRules §17.4 ([[CompactDisplay]] "can in some cases
// influence plural form selection" — a hedge, not a mandate; CLDR selection is
// operand-based). sec-intl.pluralrules.prototype.select,
// sec-intl.pluralrules.prototype.selectrange,
// sec-intl.pluralrules.prototype.resolvedoptions

var locales = ['en', 'fr', 'pl', 'ru', 'cs'];
var values = [0, 1, 1.5, 2, 1000, 1100000, 1300000, 2000000, 1000000000];
var ranges = [[1, 1000000], [1, 2000000], [1000, 2000], [1, 1100000]];

for (var li = 0; li < locales.length; li++) {
  var loc = locales[li];
  var prShort = new Intl.PluralRules(loc, {notation: 'compact', compactDisplay: 'short'});
  var prLong = new Intl.PluralRules(loc, {notation: 'compact', compactDisplay: 'long'});

  // resolvedOptions surfaces the option exactly as supplied (issue #171).
  if (prShort.resolvedOptions().compactDisplay !== 'short') {
    throw new Test262Error(
      loc + ': resolvedOptions().compactDisplay should be "short", got "' +
      prShort.resolvedOptions().compactDisplay + '"');
  }
  if (prLong.resolvedOptions().compactDisplay !== 'long') {
    throw new Test262Error(
      loc + ': resolvedOptions().compactDisplay should be "long", got "' +
      prLong.resolvedOptions().compactDisplay + '"');
  }

  // select: short and long compact display yield the same category.
  for (var vi = 0; vi < values.length; vi++) {
    var v = values[vi];
    var s = prShort.select(v);
    var l = prLong.select(v);
    if (s !== l) {
      throw new Test262Error(
        loc + ' select(' + v + '): compactDisplay must not change category; ' +
        'short="' + s + '" long="' + l + '"');
    }
  }

  // selectRange: short and long compact display yield the same category.
  for (var ri = 0; ri < ranges.length; ri++) {
    var a = ranges[ri][0];
    var b = ranges[ri][1];
    var rs = prShort.selectRange(a, b);
    var rl = prLong.selectRange(a, b);
    if (rs !== rl) {
      throw new Test262Error(
        loc + ' selectRange(' + a + ',' + b + '): compactDisplay must not change ' +
        'category; short="' + rs + '" long="' + rl + '"');
    }
  }
}
