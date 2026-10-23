// Tests that Intl.DisplayNames resolves names for various types.
// Spec: ECMA-402, sec-intl-displaynames-objects

// Currency lookups
var dnCurrency = new Intl.DisplayNames(['en'], {type: 'currency'});
var usd = dnCurrency.of('USD');
if (typeof usd !== 'string' || usd.length === 0) {
  throw new Test262Error('currency USD should return a non-empty string, got: ' + usd);
}
var eur = dnCurrency.of('EUR');
if (typeof eur !== 'string' || eur.length === 0) {
  throw new Test262Error('currency EUR should return a non-empty string, got: ' + eur);
}
var jpy = dnCurrency.of('JPY');
if (typeof jpy !== 'string' || jpy.length === 0) {
  throw new Test262Error('currency JPY should return a non-empty string, got: ' + jpy);
}

// Calendar lookups
var dnCal = new Intl.DisplayNames(['en'], {type: 'calendar'});
var greg = dnCal.of('gregory');
if (typeof greg !== 'string' || greg.length === 0) {
  throw new Test262Error('calendar gregory should return a non-empty string, got: ' + greg);
}
var iso = dnCal.of('iso8601');
if (typeof iso !== 'string' || iso.length === 0) {
  throw new Test262Error('calendar iso8601 should return a non-empty string, got: ' + iso);
}

// DateTimeField lookups
var dnField = new Intl.DisplayNames(['en'], {type: 'dateTimeField'});
var fields = ['era', 'year', 'quarter', 'month', 'weekOfYear', 'weekday', 'day', 'dayPeriod', 'hour', 'minute', 'second', 'timeZoneName'];
for (var i = 0; i < fields.length; i++) {
  var result = dnField.of(fields[i]);
  if (typeof result !== 'string' || result.length === 0) {
    throw new Test262Error('dateTimeField ' + fields[i] + ' should return a non-empty string, got: ' + result);
  }
}

// Script lookups
var dnScript = new Intl.DisplayNames(['en'], {type: 'script'});
var latn = dnScript.of('Latn');
if (typeof latn !== 'string' || latn.length === 0) {
  throw new Test262Error('script Latn should return a non-empty string, got: ' + latn);
}
var cyrl = dnScript.of('Cyrl');
if (typeof cyrl !== 'string' || cyrl.length === 0) {
  throw new Test262Error('script Cyrl should return a non-empty string, got: ' + cyrl);
}

// Language lookups
var dnLang = new Intl.DisplayNames(['en'], {type: 'language'});
var en = dnLang.of('en');
if (typeof en !== 'string' || en.length === 0) {
  throw new Test262Error('language en should return a non-empty string, got: ' + en);
}
var fr = dnLang.of('fr');
if (typeof fr !== 'string' || fr.length === 0) {
  throw new Test262Error('language fr should return a non-empty string, got: ' + fr);
}

// Region lookups
var dnRegion = new Intl.DisplayNames(['en'], {type: 'region'});
var us = dnRegion.of('US');
if (typeof us !== 'string' || us.length === 0) {
  throw new Test262Error('region US should return a non-empty string, got: ' + us);
}

// Fallback 'code' returns the code itself for unknown codes
var dnCode = new Intl.DisplayNames(['en'], {type: 'region', fallback: 'code'});
var codeResult = dnCode.of('US');
if (typeof codeResult !== 'string' || codeResult.length === 0) {
  throw new Test262Error('fallback code for known region should return string, got: ' + codeResult);
}
