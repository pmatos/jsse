// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
var o = {}, five = 5, nul = null, und, arr = [], fn = function(){}, x, f = () => null, t = () => 1;
var a = {"]": 1};
const cases = {
 i1: () => { Object.getOwnPropertyDescriptor(Intl.NumberFormat.prototype, 'format').get.call({}); },
 i2: () => { Object.getOwnPropertyDescriptor(Intl.DateTimeFormat.prototype, 'format').get.call(Intl.DateTimeFormat.prototype); },
 i3: () => { Intl.NumberFormat.prototype.formatToParts.call({}, 1); },
 i4: () => { Intl.NumberFormat.prototype.resolvedOptions.call({}); },
 i5: () => { Intl.ListFormat.prototype.format.call({}, []); },
 i6: () => { Intl.ListFormat.prototype.formatToParts.call({}, []); },
 i7: () => { Intl.ListFormat.prototype.resolvedOptions.call({}); },
 i8: () => { Intl.DisplayNames.prototype.of.call({}, 'en'); },
 i9: () => { Intl.DisplayNames.prototype.resolvedOptions.call({}); },
 i10: () => { Intl.Segmenter.prototype.segment.call({}, 'a'); },
 i11: () => { Intl.Segmenter.prototype.resolvedOptions.call({}); },
 i12: () => { Intl.DateTimeFormat.prototype.formatToParts.call({}, 1); },
 i13: () => { Intl.DateTimeFormat.prototype.resolvedOptions.call({}); },
 i14: () => { Intl.DateTimeFormat.prototype.formatRange.call({}, 1, 2); },
 i15: () => { Intl.NumberFormat.prototype.formatRange.call({}, 1, 2); },
 i16: () => { Intl.PluralRules.prototype.select.call({}, 1); },
 i17: () => { Intl.PluralRules.prototype.resolvedOptions.call({}); },
 i18: () => { Intl.RelativeTimeFormat.prototype.format.call({}, 1, 'day'); },
 i19: () => { Intl.RelativeTimeFormat.prototype.resolvedOptions.call({}); },
 i20: () => { Object.getOwnPropertyDescriptor(Intl.Collator.prototype, 'compare').get.call({}); },
 i21: () => { Intl.Collator.prototype.resolvedOptions.call({}); },
 i22: () => { Intl.Locale.prototype.maximize.call({}); },
 i23: () => { Object.getOwnPropertyDescriptor(Intl.Locale.prototype, 'language').get.call({}); },
 i25: () => { Intl.PluralRules.prototype.selectRange.call({}, 1, 2); },
 i26: () => { Intl.Segmenter.prototype.segment.call(1, 'a'); },
 i27: () => { Intl.ListFormat.prototype.format.call(undefined, []); },
 i28: () => { Intl.NumberFormat.prototype.formatRangeToParts.call({}, 1, 2); },
 i29: () => { Intl.DateTimeFormat.prototype.formatRangeToParts.call({}, 1, 2); },
 i30: () => { Intl.Segmenter.prototype.segment.call(Intl.Segmenter.prototype, 'a'); },
 r1: () => { new RegExp('(?<=a)+', 'u'); },
 r2: () => { new RegExp('(?<=a)+'); },
 r3: () => { new RegExp('(?<=a)*', 'u'); },
 r4: () => { new RegExp('(?<!a)+', 'u'); },
 r5: () => { new RegExp('(?=a)+', 'u'); },
 r6: () => { new RegExp('(?=a)+'); },
 r7: () => { new RegExp('(?<=a){2}', 'u'); },
 r8: () => { new RegExp('(?<=a)?', 'u'); },
 r9: () => { new RegExp('(?<=a)+', 'v'); },
 r10: () => { new RegExp('(?!a)*', 'u'); },
};
(async () => {
for (const [k, fnc] of Object.entries(cases)) {
  try { await fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
})();
