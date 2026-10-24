/*---
description: >
  Repeated and interleaved Intl.NumberFormat / Intl.DateTimeFormat format calls
  on instances with different locales and options keep each instance's own
  resolved options.
esid: sec-intl.numberformat.prototype.format
features: [Intl]
info: |
  Intl.NumberFormat.prototype.format and Intl.DateTimeFormat.prototype.format
  format with the [[Locale]] and option slots of the instance they were read
  from, regardless of which other instances were formatted before.
---*/

var nfEn = new Intl.NumberFormat("en-US");
var nfDe = new Intl.NumberFormat("de-DE");
var nfNoGroup = new Intl.NumberFormat("en-US", { useGrouping: false });
var nfMin2 = new Intl.NumberFormat("en-US", { useGrouping: "min2" });

for (var i = 0; i < 3; i++) {
  assert.sameValue(nfEn.format(1234567.5), "1,234,567.5", "en-US");
  assert.sameValue(nfDe.format(1234567.5), "1.234.567,5", "de-DE");
  assert.sameValue(nfNoGroup.format(1234567.5), "1234567.5", "useGrouping false");
  assert.sameValue(nfMin2.format(1234), "1234", "min2 below threshold");
  assert.sameValue(nfMin2.format(12345), "12,345", "min2 above threshold");
}

var utc = 978307200000;
var dtfNumeric = new Intl.DateTimeFormat("en-US", { timeZone: "UTC" });
var dtfLong = new Intl.DateTimeFormat("en-US", { timeZone: "UTC", dateStyle: "long" });
var dtfGb = new Intl.DateTimeFormat("en-GB", { timeZone: "UTC" });
var dtfTime = new Intl.DateTimeFormat("en-US", { timeZone: "UTC", hour: "numeric", minute: "numeric", hour12: false });

for (var j = 0; j < 3; j++) {
  assert.sameValue(dtfNumeric.format(utc), "1/1/2001", "en-US numeric");
  assert.sameValue(dtfLong.format(utc), "January 1, 2001", "en-US long");
  assert.sameValue(dtfGb.format(utc), "01/01/2001", "en-GB numeric");
  assert.sameValue(dtfTime.format(utc), "00:00", "en-US 24h time");
}
