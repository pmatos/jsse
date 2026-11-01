// V8 text of Intl receivers that are not an Intl object, including DurationFormat and
// Segments (#1568). Run verbatim on Node and on `jsse --node`.
const cases = {
 d1: () => Intl.DurationFormat.prototype.format.call({}, {}),
 d2: () => Intl.DurationFormat.prototype.formatToParts.call({}, {}),
 d3: () => Intl.DurationFormat.prototype.resolvedOptions.call({}),
 d4: () => Intl.DurationFormat.prototype.format.call(1, {}),
 s1: () => new Intl.Segmenter().segment('a').containing.call({}, 0),
 s2: () => Intl.Segmenter.prototype.segment.call(undefined, 'a'),
 c1: () => Object.getOwnPropertyDescriptor(Intl.Collator.prototype, 'compare').get.call(1),
 c2: () => Object.getOwnPropertyDescriptor(Intl.Collator.prototype, 'compare').get.call(undefined),
 p1: () => Intl.PluralRules.prototype.select.call(1, 1),
 p2: () => Intl.PluralRules.prototype.select.call(undefined, 1),
 l1: () => Intl.Locale.prototype.toString.call({}),
 l2: () => Object.getOwnPropertyDescriptor(Intl.Locale.prototype, 'baseName').get.call({}),
 l3: () => Intl.Locale.prototype.getCalendars.call({}),
 l4: () => Object.getOwnPropertyDescriptor(Intl.Locale.prototype, 'calendars').get.call({}),
 r1: () => Intl.RelativeTimeFormat.prototype.formatToParts.call({}, 1, 'day'),
 r2: () => Intl.ListFormat.prototype.formatToParts.call(1, []),
 n1: () => Intl.NumberFormat.prototype.formatToParts.call(undefined, 1),
 n2: () => Object.getOwnPropertyDescriptor(Intl.NumberFormat.prototype, 'format').get.call(Object.create(Intl.NumberFormat.prototype)),
 n3: () => Object.getOwnPropertyDescriptor(Intl.DateTimeFormat.prototype, 'format').get.call(function () {}),
 n4: () => Intl.DateTimeFormat.prototype.resolvedOptions.call([]),
 n5: () => Intl.NumberFormat.prototype.resolvedOptions.call(new Intl.Collator()),
 n6: () => Intl.DisplayNames.prototype.of.call(new Intl.Collator(), 'en'),
 n7: () => Intl.getCanonicalLocales.call(1, {}),
};
for (const [k, fnc] of Object.entries(cases)) {
  try { fnc(); console.log(k, '=> no error'); } catch (e) { console.log(k, '=>', e && e.constructor && e.constructor.name, e && e.message); }
}
