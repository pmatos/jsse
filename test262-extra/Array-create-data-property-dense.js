// Dense Array results created with CreateDataPropertyOrThrow must preserve
// ordinary own data property semantics, including present undefined values.
// Spec: sec-createdatapropertyorthrow, sec-array.prototype.map,
//       sec-array.prototype.filter, sec-array.prototype.slice.

var mapped = [1, 2, 3].map(function (value, index) {
  return index === 1 ? undefined : value * 2;
});
if (mapped.length !== 3 || mapped[0] !== 2 || mapped[1] !== undefined || mapped[2] !== 6) {
  throw new Test262Error('map result values or length are wrong');
}
var keys = Object.keys(mapped);
if (keys.length !== 3 || keys[0] !== '0' || keys[1] !== '1' || keys[2] !== '2') {
  throw new Test262Error('map result must have three enumerable own indices');
}
for (var i = 0; i < 3; i++) {
  var desc = Object.getOwnPropertyDescriptor(mapped, String(i));
  if (!desc || !desc.writable || !desc.enumerable || !desc.configurable) {
    throw new Test262Error('map result index ' + i + ' must be a default data property');
  }
}

var filtered = [1, 2, 3, 4].filter(function (value) { return value % 2 === 0; });
if (filtered.length !== 2 || filtered[0] !== 2 || filtered[1] !== 4) {
  throw new Test262Error('filter must create dense result indices');
}

var sliced = [1, , 3].slice(0);
if (sliced.length !== 3 || sliced[0] !== 1 || 1 in sliced || sliced[2] !== 3) {
  throw new Test262Error('slice must preserve holes between created indices');
}

delete mapped[0];
if (0 in mapped || mapped.length !== 3) {
  throw new Test262Error('a dense result index must remain configurable');
}
