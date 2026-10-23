// Intl.PluralRules.prototype.selectRange must compute its endpoint plural
// categories using the instance's `notation` (e.g. "compact"), exactly as
// `select` does. Regression test for selectRange ignoring `notation`
// (issue #174): selectRange built operands via the standard-notation path, so
// compact ranges were categorized from the standard integer value instead of
// the compact form (which carries the compact exponent operand `c`).
//
// Spec: ECMA-402 ResolvePluralRange computes the plural category of each
// endpoint via ResolvePlural using the object's [[Notation]] (and
// [[CompactDisplay]]), then PluralRuleSelectRange combines them.
// sec-resolvepluralrange, sec-intl.pluralrules.prototype.selectrange

// Oracle-free invariant: for finite v, selectRange(v, v) === select(v).
// (CLDR/ICU: category_for_range(ops, ops) resolves to category_for(ops).)
// This holds only when selectRange builds operands the same way select does.
// We use `fr` values where the COMPACT plural category ("many", via e != 0..5)
// differs from the STANDARD category ("other", since i % 1000000 != 0), so the
// pre-fix selectRange (standard operands) returns a different category than
// select (compact operands).

var values = [1100000, 1300000, 1700000, 2300000];

var prCompact = new Intl.PluralRules('fr', {notation: 'compact'});
var prStandard = new Intl.PluralRules('fr', {notation: 'standard'});

for (var i = 0; i < values.length; i++) {
  var v = values[i];

  // Guard: confirm v is notation-discriminating for this CLDR data. If this
  // ever stops holding (data drift), fail loudly instead of passing vacuously —
  // pick another X.Y-million value where the compact "many" rule diverges.
  var cCat = prCompact.select(v);
  var sCat = prStandard.select(v);
  if (cCat === sCat) {
    throw new Test262Error(
      'fr ' + v + ': expected compact and standard plural categories to differ ' +
      '(test no longer discriminating); both = ' + cCat);
  }

  // Core invariant (the fix): selectRange must use the compact operands.
  var rangeCat = prCompact.selectRange(v, v);
  if (rangeCat !== cCat) {
    throw new Test262Error(
      'fr ' + v + ': selectRange(v,v) must honour notation:"compact" and equal ' +
      'select(v)="' + cCat + '", got "' + rangeCat + '"');
  }

  // Positive control: standard notation is already consistent (holds before and
  // after the fix); confirms the selectRange(v,v)===select(v) invariant itself.
  var stdRangeCat = prStandard.selectRange(v, v);
  if (stdRangeCat !== sCat) {
    throw new Test262Error(
      'fr ' + v + ': standard selectRange(v,v) should equal select(v)="' + sCat +
      '", got "' + stdRangeCat + '"');
  }
}
