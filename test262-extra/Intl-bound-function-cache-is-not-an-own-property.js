/*---
description: >
  The bound format/compare function that Intl.NumberFormat, Intl.DateTimeFormat
  and Intl.Collator cache on the instance is an internal slot, not a property.
esid: sec-properties-of-intl-numberformat-instances
features: [Intl, Reflect]
info: |
  Intl.NumberFormat.prototype.format and Intl.DateTimeFormat.prototype.format
  return the [[BoundFormat]] internal slot, creating it on first access;
  Intl.Collator.prototype.compare does the same with [[BoundCompare]]. Internal
  slots are not properties, so no own key may appear and no property of that
  name may be observed or overwritten.
includes: [compareArray.js]
---*/

[
  [new Intl.NumberFormat(), "format", "[[BoundFormat]]"],
  [new Intl.DateTimeFormat(), "format", "[[BoundFormat]]"],
  [new Intl.Collator(), "compare", "[[BoundCompare]]"],
].forEach(function (entry) {
  var instance = entry[0];
  var accessor = entry[1];
  var slot = entry[2];
  var label = Object.prototype.toString.call(instance);

  var first = instance[accessor];
  assert.sameValue(typeof first, "function", label + " accessor result");
  assert.sameValue(instance[accessor], first, label + " is cached");

  assert.compareArray(Reflect.ownKeys(instance), [], label + " own keys");
  assert.sameValue(Object.prototype.hasOwnProperty.call(instance, slot), false, label + " hasOwnProperty");
  assert.sameValue(instance[slot], undefined, label + " read of the slot name");

  var seen = [];
  for (var key in instance) seen.push(key);
  assert.compareArray(seen, [], label + " for-in");

  instance[slot] = "user value";
  assert.sameValue(instance[slot], "user value", label + " user property of the same name");
  assert.sameValue(instance[accessor], first, label + " cache unaffected by the user property");
  assert.compareArray(Reflect.ownKeys(instance), [slot], label + " only the user property");
});
